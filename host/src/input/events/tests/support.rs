//! Test helpers: read a scratch-file device's output as evdev frames, make a device over a file.

use crate::input::*;
use std::fs::OpenOptions;

/// The event frames (split at SYN_REPORT) a device wrote into `path`.
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

/// Creates a `UInputDevice` that writes to a temp file instead of /dev/uinput; returns it and the path.
pub(super) fn device_over_file(name: &str) -> (UInputDevice, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("uscreen-{}-{}", name, std::process::id()));
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .unwrap();
    (UInputDevice { file, held: 0 }, path)
}

/// A touchpad finger at (`x`, `y`) millimetres with the given id.
pub(super) fn finger(id: i32, x: f64, y: f64) -> PadFinger {
    PadFinger {
        id,
        x_mm: x,
        y_mm: y,
        pressure: 0.5,
    }
}
