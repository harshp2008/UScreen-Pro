//! Touchpad mode: the tablet recognises gestures and sends relative motion, buttons, scroll, raw
//! fingers (native gestures) or named gestures; this turns them into mouse/keyboard/multi-touch
//! events. Also the wake nudge that brings back a hidden cursor.

use super::*;

/// High-resolution wheel units per tablet pixel of two-finger travel: one notch (120 units) per
/// 40 px, about what a laptop touchpad feels like.
const SCROLL_UNITS_PER_PX: f64 = 3.0;

/// Wheel units per tablet pixel of pinch travel (Ctrl + wheel zooms): a 100 px pinch is about five
/// notches.
const PINCH_UNITS_PER_PX: f64 = 6.0;

/// Raw fingers for native gestures (only in Touchpad mode with native gestures on).
pub(super) fn handle_touchpad_raw(
    instance: u32,
    slots: Vec<RawSlot>,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
) {
    // Only while native gestures are in use in Touchpad mode: an old or confused tablet
    // must not bring a touchpad into existence in Stylus or Display mode.
    let opts = crate::config::options::current_options(instance);
    if opts.input_mode != crate::config::InputMode::Touchpad || !opts.native_gestures_effective() {
        return;
    }
    let fingers: Vec<PadFinger> = slots
        .iter()
        .map(|s| PadFinger {
            id: s.id,
            x_mm: s.x,
            y_mm: s.y,
            pressure: s.pressure,
        })
        .collect();
    if let Ok(mut guard) = uinput.lock() {
        guard.touchpad_raw(&fingers);
    }
}

/// Relative pointer motion in tablet pixels; the sub-pixel remainder is kept for the next sample.
pub(super) fn handle_rel(dx: f64, dy: f64, uinput: &Arc<std::sync::Mutex<InjectDevices>>) {
    if let Ok(mut guard) = uinput.lock() {
        // Keep the fraction: slow movements are often well under one pixel per sample
        // and would otherwise never move at all.
        let fx = guard.rel_rest.0 + dx;
        let fy = guard.rel_rest.1 + dy;
        let (ix, iy) = (fx.trunc(), fy.trunc());
        guard.rel_rest = (fx - ix, fy - iy);
        if let Some(dev) = guard.mouse() {
            if let Err(e) = dev.move_by(ix as i32, iy as i32) {
                warn!("Failed to move the touchpad pointer: {}", e);
            }
        }
    }
}

/// A touchpad mouse button (0 left, 1 right, 2 middle).
pub(super) fn handle_button(button: u8, down: bool, uinput: &Arc<std::sync::Mutex<InjectDevices>>) {
    if let Ok(mut guard) = uinput.lock() {
        let bit = match button {
            1 => 2u8,
            2 => 4,
            _ => 1,
        };
        if let Some(dev) = guard.mouse() {
            if dev.press(button, down).is_ok() {
                if down {
                    guard.mouse_buttons |= bit
                } else {
                    guard.mouse_buttons &= !bit
                }
            }
        }
    }
}

/// Two-finger scroll in tablet pixels, sent as high-resolution wheel units.
pub(super) fn handle_scroll(dx: f64, dy: f64, uinput: &Arc<std::sync::Mutex<InjectDevices>>) {
    if let Ok(mut guard) = uinput.lock() {
        let ux = (dx * SCROLL_UNITS_PER_PX) as i32;
        let uy = (dy * SCROLL_UNITS_PER_PX) as i32;
        let mut rest = guard.scroll_rest;
        if let Some(dev) = guard.mouse() {
            if let Err(e) = dev.scroll_by(ux, uy, &mut rest) {
                warn!("Failed to scroll: {}", e);
            }
        }
        guard.scroll_rest = rest;
    }
}

/// A named gesture that needs keys on the computer (pinch, overview, workspace switch).
pub(super) fn handle_gesture(
    kind: &str,
    amount: f64,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
) {
    if let Ok(mut guard) = uinput.lock() {
        match kind {
            // Ctrl + wheel zooms in browsers and most viewers; fingers apart zoom in.
            "pinch" => guard.pinch_step((amount * PINCH_UNITS_PER_PX) as i32),
            "pinch_end" => guard.end_pinch(),
            // Super alone: the activities overview.
            "overview" => guard.tap_key_combo(&[KEY_LEFTMETA]),
            "workspace_prev" => guard.tap_key_combo(&[KEY_LEFTMETA, KEY_PAGEUP]),
            "workspace_next" => guard.tap_key_combo(&[KEY_LEFTMETA, KEY_PAGEDOWN]),
            other => warn!("Unknown gesture {:?}", other),
        }
    }
}

/// The user touched the tablet: count it as use and, in touchpad mode, wake the cursor.
pub(super) fn handle_wake(instance: u32, uinput: &Arc<std::sync::Mutex<InjectDevices>>) {
    // The user touched the tablet (the app swallowed that touch to wake the screen, or a
    // finger came down on the touchpad). Count it as use and, in touchpad mode, wake the
    // host cursor: Mutter hides an idle pointer, and a 1 px out-and-back brings it back.
    crate::config::options::note_activity(instance);
    // Not with native gestures: the virtual touchpad's own motion shows the cursor, and a
    // second device twitching beside it only confuses the compositor.
    let opts = crate::config::options::current_options(instance);
    if opts.input_mode == crate::config::InputMode::Touchpad && !opts.native_gestures_effective() {
        if let Ok(mut guard) = uinput.lock() {
            if let Some(dev) = guard.mouse() {
                let _ = dev.move_by(1, 0);
                let _ = dev.move_by(-1, 0);
            }
        }
    }
}
