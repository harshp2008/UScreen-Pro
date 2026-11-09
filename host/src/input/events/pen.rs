//! Pen samples: the counters used to diagnose lost hover events, the conversion of the tablet's
//! normalised sample into injector units, and `apply_pen`, which routes a sample by the device's
//! input style (auto, mouse, or plain pressure pen).

use super::*;
use crate::input::ghost::{route_to_pen, session_is_x11};

/// Counts of pen actions received, logged periodically. Hover in particular is
/// easy to lose somewhere between the tablet's view hierarchy and here, and
/// without a count there is no way to tell "not sent" from "sent but ignored".
static PEN_ACTIONS: std::sync::Mutex<[u32; 8]> = std::sync::Mutex::new([0; 8]);
static PEN_LOG_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Count one pen action and, at most every three seconds, log the counts.
pub(in crate::input) fn note_pen_action(action: u8) {
    if let Ok(mut c) = PEN_ACTIONS.lock() {
        c[(action as usize).min(7)] += 1;
    }
    let Ok(mut last) = PEN_LOG_AT.lock() else {
        return;
    };
    let now = std::time::Instant::now();
    match *last {
        Some(t) if t.elapsed().as_secs() < 3 => return,
        _ => *last = Some(now),
    }
    if let Ok(mut c) = PEN_ACTIONS.lock() {
        if c.iter().any(|&n| n > 0) {
            info!(
                "Pen events: down={} up={} move={} hover={} hover_exit={} btn_down={} btn_up={}",
                c[0], c[1], c[2], c[3], c[4], c[5], c[6]
            );
            *c = [0; 8];
        }
    }
}

/// One pen sample from the Android app: convert to the injector's units and route it by style.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_pen(
    instance: u32,
    x: f64,
    y: f64,
    pressure: f64,
    tilt_x: f64,
    tilt_y: f64,
    eraser: bool,
    action: u8,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
) {
    let abs_x = (x * COORD_MAX as f64) as i32;
    let abs_y = (y * COORD_MAX as f64) as i32;
    note_pen_action(action);
    let abs_pressure = (pressure * 4096.0) as i32;
    // Already degrees, as the tablet computes them. This used to
    // multiply by 180/π on the assumption they were radians, which
    // squashed a pen laid flat at 90° down to 57°.
    let tilt_x_deg = (tilt_x.round() as i32).clamp(-90, 90);
    let tilt_y_deg = (tilt_y.round() as i32).clamp(-90, 90);

    let opts = crate::config::options::current_options(instance);
    let sample = PenSample {
        x: abs_x,
        y: abs_y,
        pressure: abs_pressure,
        tilt_x: tilt_x_deg,
        tilt_y: tilt_y_deg,
        eraser,
        action,
    };
    if let Ok(mut guard) = uinput.lock() {
        apply_pen(&mut guard, &opts, &sample);
    }
}

/// Route one pen sample (from the Android pen or a physical tablet) according to the device's
/// options: `auto` is the unified pen (tablet data always, mouse clicks for non-art apps), `mouse` (Stylus mode) is the pen as a mouse, anything else is
/// the plain pressure pen.
pub(in crate::input) fn apply_pen(
    guard: &mut InjectDevices,
    opts: &crate::config::options::DeviceOptions,
    s: &PenSample,
) {
    let (abs_x, abs_y, action, eraser) = (s.x, s.y, s.action, s.eraser);
    // Ghost tracking: both devices follow the pen. Not on X11, where the tablet already drives the
    // one core pointer, and not without the absolute pointer.
    if opts.stylus_style == "auto" && guard.has_pointer() && !session_is_x11() {
        let to_pen = route_to_pen(&opts.stylus_art_apps);
        guard.auto_pen(
            s,
            to_pen,
            &opts.stylus_button_primary,
            &opts.stylus_button_secondary,
        );
        return;
    }
    // Stylus mode as a mouse: the pen is the pointer, its tip the left button.
    if opts.input_mode == crate::config::InputMode::Stylus
        && opts.stylus_style == "mouse"
        && guard.has_pointer()
    {
        if !eraser || action >= 4 {
            guard.pen_as_mouse(
                abs_x,
                abs_y,
                action,
                &opts.stylus_button_primary,
                &opts.stylus_button_secondary,
            );
        }
        if matches!(action, 0..=3) {
            guard.last_pen_pos = (abs_x, abs_y);
        }
        return;
    }
    let ok = if let Some(ref mut dev) = guard.pen {
        match dev.inject_pen(abs_x, abs_y, s.pressure, s.tilt_x, s.tilt_y, action, eraser) {
            Ok(_) => true,
            Err(e) => {
                warn!("Failed to inject pen: {}", e);
                false
            }
        }
    } else {
        match action {
            0 => info!(
                "Pen DOWN at ({}, {}), eraser={}, tilt=({},{})",
                abs_x, abs_y, eraser, s.tilt_x, s.tilt_y
            ),
            1 => info!("Pen UP   at ({}, {})", abs_x, abs_y),
            _ => {}
        }
        false
    };
    if ok {
        // Lift included: its position is the last the pen was seen at.
        if matches!(action, 0..=3) {
            guard.last_pen_pos = (abs_x, abs_y);
        }
        // Leaving proximity hides the tablet cursor, so hand the position to the plain pointer and
        // let an ordinary cursor stay where the pen last was.
        if action == 4 {
            guard.park_pointer();
        }
        match action {
            0 | 3 => guard.pen_proximity = true,
            1 | 4 => guard.pen_proximity = false,
            5 => guard.pen_button = true,
            6 => guard.pen_button = false,
            _ => {}
        }
    }
}
