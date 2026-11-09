//! Test helpers shared by the pen-routing tests: a rig of two scratch-file devices (a pen and a
//! pointer), a reader that turns what was written into evdev frames, and small builders.

use super::super::*;
use crate::desktop::AppIdentity;
use std::fs::OpenOptions;

pub(super) fn app(id: &str, class: &str) -> AppIdentity {
    AppIdentity {
        app_id: id.into(),
        wm_class: class.into(),
        gtk_id: String::new(),
    }
}

pub(super) fn art() -> Vec<String> {
    crate::config::default_art_apps()
}

pub(super) fn rig() -> (InjectDevices, std::path::PathBuf, std::path::PathBuf) {
    // Tests run in parallel threads of one process: every rig needs its own files.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mk = |n: &str| {
        let path =
            std::env::temp_dir().join(format!("uscreen-ghost-{}-{}-{}", n, std::process::id(), id));
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        (UInputDevice { file, held: 0 }, path)
    };
    let (pen, pp) = mk("pen");
    let (pointer, qp) = mk("pointer");
    let mut d = InjectDevices::headless();
    d.pen = Some(pen);
    d.pointer = Some(pointer);
    (d, pp, qp)
}

pub(super) fn frames(path: &std::path::Path) -> Vec<Vec<(u16, u16, i32)>> {
    let bytes = std::fs::read(path).unwrap();
    let (mut all, mut frame) = (Vec::new(), Vec::new());
    for ev in bytes
        .as_chunks::<{ std::mem::size_of::<LinuxInputEvent>() }>()
        .0
    {
        let kind = u16::from_ne_bytes(ev[16..18].try_into().unwrap());
        let code = u16::from_ne_bytes(ev[18..20].try_into().unwrap());
        let value = i32::from_ne_bytes(ev[20..24].try_into().unwrap());
        if kind == EV_SYN {
            all.push(std::mem::take(&mut frame));
        } else {
            frame.push((kind, code, value));
        }
    }
    all
}

pub(super) fn sample(action: u8, x: i32) -> PenSample {
    PenSample {
        x,
        y: 500,
        pressure: 2000,
        tilt_x: 3,
        tilt_y: -3,
        eraser: false,
        action,
    }
}

pub(super) fn has(f: &[Vec<(u16, u16, i32)>], e: (u16, u16, i32)) -> bool {
    f.iter().any(|fr| fr.contains(&e))
}

pub(super) fn done(p: &[&std::path::Path]) {
    for x in p {
        let _ = std::fs::remove_file(x);
    }
}

pub(super) fn auto(d: &mut InjectDevices, a: u8, x: i32, to_pen: bool) {
    d.auto_pen(&sample(a, x), to_pen, "right", "middle");
}

/// The `BTN_LEFT` transitions the kernel would pass on: it drops a key event that repeats the
/// current value (the lift's belt-and-braces release of an already released button).
pub(super) fn left_events(ptr: &[Vec<(u16, u16, i32)>]) -> Vec<i32> {
    let mut v: Vec<i32> = ptr
        .iter()
        .flatten()
        .filter(|e| e.0 == EV_KEY && e.1 == BTN_LEFT)
        .map(|e| e.2)
        .collect();
    v.dedup();
    v
}

pub(super) fn xs(f: &[Vec<(u16, u16, i32)>]) -> Vec<i32> {
    f.iter()
        .flat_map(|fr| fr.iter().filter(|e| e.1 == ABS_X).map(|e| e.2))
        .collect()
}
