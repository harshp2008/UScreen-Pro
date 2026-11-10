//! Touchscreen contacts: scale the tablet's normalised position and pressure and write them to the
//! virtual touchscreen. Ignored in Stylus mode, where fingers do nothing.

use super::*;

/// One touch contact (`action` 0 down, 1 up, 2 move) on multi-touch `slot`.
pub(super) fn handle_touch(
    instance: u32,
    x: f64,
    y: f64,
    pressure: f64,
    action: u8,
    slot: u8,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
) {
    // Stylus mode is a graphics tablet: fingers do nothing. The tablet app already keeps
    // them off the socket; this covers an older app and a mode switched mid-gesture.
    if crate::config::options::current_mode(instance) == crate::config::InputMode::Stylus {
        return;
    }
    let abs_x = (x * COORD_MAX as f64) as i32;
    let abs_y = (y * COORD_MAX as f64) as i32;
    let abs_pressure = (pressure * 4096.0) as i32;

    if let Ok(mut guard) = uinput.lock() {
        let ok = if let Some(ref mut dev) = guard.touch {
            match dev.inject_touch(abs_x, abs_y, abs_pressure, action, slot) {
                Ok(_) => true,
                Err(e) => {
                    warn!("Failed to inject touch: {}", e);
                    false
                }
            }
        } else {
            match action {
                0 => info!("Touch DOWN at ({}, {})", abs_x, abs_y),
                1 => info!("Touch UP   at ({}, {})", abs_x, abs_y),
                _ => {}
            }
            false
        };
        if ok {
            let bit = 1u16 << (slot.min(15) as u16);
            match action {
                0 => guard.active_slots |= bit,
                1 => guard.active_slots &= !bit,
                _ => {}
            }
        }
    }
}
