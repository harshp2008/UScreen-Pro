//! A small in-memory flight recorder of pen samples, for bugs that cannot be reproduced on demand
//! (a pen resting on the tablet that sometimes takes the cursor from the mouse). It keeps the last
//! few thousand notable samples; `uscreen debug-dump` asks the running daemon to write them to
//! `~/.local/share/uscreen/pen_dump.txt`.
//!
//! Hover is recorded in full while it is followed and sampled (1 in 25) while it is skipped,
//! moves 1 in 10, so a minute of hover and a long stroke do not push the interesting part out.

use super::ghost::PenSample;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const CAPACITY: usize = 4000;

struct Rec {
    ms: u64,
    action: u8,
    x: i32,
    y: i32,
    pressure: i32,
    tilt: (i32, i32),
    route_pen: bool,
    note: &'static str,
}

struct Ring {
    start: Instant,
    recs: VecDeque<Rec>,
    moves: u64,
    skips: u64,
}

fn ring() -> &'static Mutex<Ring> {
    static R: OnceLock<Mutex<Ring>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Ring { start: Instant::now(), recs: VecDeque::new(), moves: 0, skips: 0 }))
}

/// Record one sample with what the router did with it (`note`).
pub(super) fn record(s: &PenSample, route_pen: bool, note: &'static str) {
    let Ok(mut r) = ring().lock() else { return };
    match (s.action, note) {
        (2, _) => {
            r.moves += 1;
            if r.moves % 10 != 1 {
                return;
            }
        }
        (3, "skip") => {
            r.skips += 1;
            if r.skips % 25 != 1 {
                return;
            }
        }
        _ => {}
    }
    let ms = r.start.elapsed().as_millis() as u64;
    if r.recs.len() >= CAPACITY {
        r.recs.pop_front();
    }
    r.recs.push_back(Rec {
        ms,
        action: s.action,
        x: s.x,
        y: s.y,
        pressure: s.pressure,
        tilt: (s.tilt_x, s.tilt_y),
        route_pen,
        note,
    });
}

/// The recorded samples as text, oldest first, with a header.
pub(super) fn render() -> String {
    let Ok(r) = ring().lock() else { return String::new() };
    let mut out = format!(
        "# uscreen pen recorder: {} samples (hover followed in full, skipped 1/25, moves 1/10); ms since daemon start, now {}\n\
         # ms action(0 down 1 up 2 move 3 hover 4 exit 5-8 buttons) x y pressure tilt route note\n",
        r.recs.len(),
        r.start.elapsed().as_millis()
    );
    for e in &r.recs {
        out.push_str(&format!(
            "{} {} {} {} {} {},{} {} {}\n",
            e.ms,
            e.action,
            e.x,
            e.y,
            e.pressure,
            e.tilt.0,
            e.tilt.1,
            if e.route_pen { "pen" } else { "mouse" },
            e.note
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(action: u8, x: i32) -> PenSample {
        PenSample { x, y: 5, pressure: 0, tilt_x: 1, tilt_y: 2, eraser: false, action }
    }

    #[test]
    fn records_follow_in_full_and_samples_skips_and_moves() {
        let before = ring().lock().unwrap().recs.len();
        for i in 0..50 {
            record(&s(3, 900_000 + i), false, "skip");
        }
        record(&s(0, 7), true, "down");
        let text = render();
        let skips = text.lines().filter(|l| l.contains("90000") && l.ends_with("skip")).count();
        assert!((1..=3).contains(&skips), "1 in 25 of 50: {}", skips);
        assert!(text.lines().any(|l| l.contains(" 0 7 5 0 1,2 pen down")));
        assert!(ring().lock().unwrap().recs.len() <= CAPACITY && ring().lock().unwrap().recs.len() > before);
    }

    #[test]
    fn the_ring_is_bounded() {
        for i in 0..(CAPACITY as i32 + 100) {
            record(&s(1, i), false, "up");
        }
        assert_eq!(ring().lock().unwrap().recs.len(), CAPACITY);
    }
}
