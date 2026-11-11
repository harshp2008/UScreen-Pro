//! Is the pointer on the tablet's screen? Answered by the compositor, through `spawn-at query`.
//!
//! The EVDI helper also reports this from `XQueryPointer`, but on Wayland that call only follows
//! the pointer while it is over an X11 (Xwayland) window; over any native window it keeps the last X
//! position. Measured here: `xdotool getmouselocation` said 1464,540 while the compositor said
//! 1088,527. So the idle-dim timer could not see the mouse arrive on the tablet's screen and the
//! tablet stayed dimmed. This watcher asks the compositor instead, a few times a second.

use crate::capture::DetectedMode;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::watch;

/// One monitor of `spawn-at query layout`: its logical rectangle and fractional scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    /// The primary (laptop) monitor.
    pub primary: bool,
    /// Logical pixels per physical pixel's inverse: physical size = logical size x scale.
    pub scale: f64,
    /// Logical left edge.
    pub x: i32,
    /// Logical top edge.
    pub y: i32,
    /// Logical width.
    pub w: i32,
    /// Logical height.
    pub h: i32,
}

/// `1464x915@1920,81` as (x, y, w, h).
fn parse_geometry(tok: &str) -> Option<(i32, i32, i32, i32)> {
    let (size, pos) = tok.split_once('@')?;
    let (w, h) = size.split_once('x')?;
    let (x, y) = pos.split_once(',')?;
    Some((
        x.parse().ok()?,
        y.parse().ok()?,
        w.parse().ok()?,
        h.parse().ok()?,
    ))
}

/// The monitors in the table of `spawn-at query layout`. A row is read from its end (insets,
/// work area, screen, scale, primary) because the monitor's name may contain spaces.
pub fn parse_layout(text: &str) -> Vec<Monitor> {
    text.lines()
        .filter_map(|line| {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.len() < 7 || t[0].parse::<u32>().is_err() {
                return None;
            }
            let n = t.len();
            let (x, y, w, h) = parse_geometry(t[n - 3])?;
            Some(Monitor {
                primary: t[n - 5] == "yes",
                scale: t[n - 4].parse().ok()?,
                x,
                y,
                w,
                h,
            })
        })
        .collect()
}

/// `1088, 527` as the pointer's logical position.
pub fn parse_pointer(text: &str) -> Option<(i32, i32)> {
    let (x, y) = text.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// The monitor that is the tablet: the one whose physical size (logical x scale) is the virtual
/// output's mode, in either orientation. With no size match, the only non-primary monitor.
pub fn tablet_monitor(monitors: &[Monitor], mode: Option<DetectedMode>) -> Option<Monitor> {
    if let Some(m) = mode {
        let phys = |mon: &Monitor| {
            (
                (mon.w as f64 * mon.scale).round() as u32,
                (mon.h as f64 * mon.scale).round() as u32,
            )
        };
        let hit = monitors.iter().find(|mon| {
            !mon.primary && (phys(mon) == (m.width, m.height) || phys(mon) == (m.height, m.width))
        });
        if hit.is_some() {
            return hit.copied();
        }
    }
    let mut others = monitors.iter().filter(|m| !m.primary);
    match (others.next(), others.next()) {
        (Some(only), None) => Some(*only),
        _ => None,
    }
}

/// Whether `p` is inside `mon`.
pub fn inside(mon: &Monitor, p: (i32, i32)) -> bool {
    p.0 >= mon.x && p.0 < mon.x + mon.w && p.1 >= mon.y && p.1 < mon.y + mon.h
}

/// Whether this session can use the compositor's answer: GNOME on Wayland with `spawn-at`.
pub fn available() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t.eq_ignore_ascii_case("wayland"))
        && super::spawn_at::installed()
}

/// Run `spawn-at query <what>` and return its output, or `None` if it failed or timed out.
async fn query(what: &str) -> Option<String> {
    let out = tokio::time::timeout(
        Duration::from_secs(2),
        Command::new("spawn-at")
            .args(["query", what])
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Report to the idle-dim logic, for the rest of the task's life, whether the pointer is on the
/// tablet's screen. Until the tablet's monitor is found the pointer counts as being on it, so
/// nothing dims on missing information.
pub async fn watch_pointer(instance: u32, mode_rx: watch::Receiver<Option<DetectedMode>>) {
    let mut tablet: Option<Monitor> = None;
    let mut last_layout = tokio::time::Instant::now() - Duration::from_secs(60);
    let mut tick = tokio::time::interval(Duration::from_millis(150));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        // The layout changes rarely; look again every 2 s, and every tick while it is unknown.
        if tablet.is_none() || last_layout.elapsed() >= Duration::from_secs(2) {
            last_layout = tokio::time::Instant::now();
            if let Some(text) = query("layout").await {
                tablet = tablet_monitor(&parse_layout(&text), *mode_rx.borrow());
            }
        }
        let Some(mon) = tablet else {
            crate::config::options::set_pointer(instance, true);
            continue;
        };
        if let Some(p) = query("pointer").await.and_then(|t| parse_pointer(&t)) {
            crate::config::options::set_pointer(instance, inside(&mon, p));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: &str = "MON  NAME          PRIMARY  SCALE  SCREEN             WORKAREA           INSETS (T/R/B/L)\n0    -             yes      1      1920x1080@0,0      1920x1040@0,40     40/0/0/0\n1    -             no       1.748633861541748  1464x915@1920,81   1464x915@1920,81   0/0/0/0\n";

    /// The table of `spawn-at query layout` is read from its end, and the header is skipped.
    #[test]
    fn layout_rows_are_parsed() {
        let m = parse_layout(LAYOUT);
        assert_eq!(m.len(), 2);
        assert!(m[0].primary && !m[1].primary);
        assert_eq!((m[1].x, m[1].y, m[1].w, m[1].h), (1920, 81, 1464, 915));
        assert!((m[1].scale - 1.7486).abs() < 1e-3);
    }

    /// The tablet is the monitor whose physical size is the virtual output's mode; the pointer test
    /// uses its logical rectangle.
    #[test]
    fn the_tablet_is_found_by_size_and_the_pointer_by_rectangle() {
        let m = parse_layout(LAYOUT);
        let mode = Some(DetectedMode {
            width: 2560,
            height: 1600,
            refresh: 90,
        });
        let t = tablet_monitor(&m, mode).unwrap();
        assert_eq!(t.x, 1920);
        assert!(inside(&t, (2000, 100)));
        assert!(
            !inside(&t, (1088, 527)),
            "the laptop screen is not the tablet"
        );
        assert!(!inside(&t, (1919, 100)) && !inside(&t, (1920 + 1464, 100)));
        // No size match: the only non-primary monitor.
        assert_eq!(tablet_monitor(&m, None).unwrap().x, 1920);
        let wrong = Some(DetectedMode {
            width: 800,
            height: 600,
            refresh: 60,
        });
        assert_eq!(tablet_monitor(&m, wrong).unwrap().x, 1920);
    }

    /// Two other monitors and no size match: unknown, not a guess.
    #[test]
    fn several_candidates_without_a_size_match_are_not_guessed() {
        let text = format!(
            "{LAYOUT}2    -             no       1  1280x720@3384,0  1280x720@3384,0  0/0/0/0\n"
        );
        assert!(tablet_monitor(&parse_layout(&text), None).is_none());
    }

    /// The pointer line is "x, y".
    #[test]
    fn the_pointer_line_is_parsed() {
        assert_eq!(parse_pointer("1088, 527\n"), Some((1088, 527)));
        assert_eq!(parse_pointer("nonsense"), None);
    }
}
