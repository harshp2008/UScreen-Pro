//! Turning decoded [`InputEvent`]s into evdev writes on the virtual devices (touch slots, pen
//! tilt/pressure/eraser, pointer buttons, keys) plus a small counter of pen actions used to
//! diagnose lost hover events.

use super::*;

/// Counts of pen actions received, logged periodically. Hover in particular is
/// easy to lose somewhere between the tablet's view hierarchy and here, and
/// without a count there is no way to tell "not sent" from "sent but ignored".
pub(super) static PEN_ACTIONS: std::sync::Mutex<[u32; 8]> = std::sync::Mutex::new([0; 8]);
pub(super) static PEN_LOG_AT: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);

pub(super) fn note_pen_action(action: u8) {
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

/// High-resolution wheel units per tablet pixel of two-finger travel: one notch (120 units) per
/// 40 px, about what a laptop touchpad feels like.
const SCROLL_UNITS_PER_PX: f64 = 3.0;

/// Wheel units per tablet pixel of pinch travel (Ctrl + wheel zooms) and of rotation arc
/// (Alt + wheel): a 100 px pinch is about five notches.
const PINCH_UNITS_PER_PX: f64 = 6.0;
const ROTATE_UNITS_PER_PX: f64 = 4.0;

use crate::stylus_profile::{self as profiles, HostAction, StylusButton, StylusEvent};

/// Run the active profile's action for each event; side buttons the profile does not map pass
/// through to the virtual pen as `BTN_STYLUS`, exactly as before profiles existed.
fn dispatch_stylus(
    guard: &mut InjectDevices,
    events: Vec<StylusEvent>,
    profile: &dyn profiles::StylusProfile,
) {
    for ev in events {
        if let Some(action) = profile.map(&ev) {
            info!("Stylus {:?} -> {}", ev.trigger().unwrap_or("?"), action);
            guard.run_action(&action);
        }
    }
}

/// The profile that applies to this tablet's pen right now. In Touchpad mode no profile applies:
/// the pen keeps its plain behaviour and nothing a profile maps can fire.
fn active_profile(instance: u32) -> Arc<dyn profiles::StylusProfile> {
    if crate::options::current_mode(instance) == crate::config::InputMode::Touchpad {
        Arc::new(profiles::StandardProfile)
    } else {
        profiles::resolve(&crate::options::current_stylus_profile(instance))
    }
}

/// The primary stylus button (pen action 5 = down, 6 = up).
fn primary_button(profile: &dyn profiles::StylusProfile, down: bool, guard: &mut InjectDevices) {
    if down {
        if let Some(action) = profile.map(&StylusEvent::ButtonPress(StylusButton::Primary)) {
            info!("Stylus button_primary -> {}", action);
            guard.pen_button_consumed = true;
            guard.run_action(&action);
            return;
        }
    } else if guard.pen_button_consumed {
        guard.pen_button_consumed = false;
        return;
    }
    if let Some(ref mut dev) = guard.pen {
        if dev
            .inject_pen(0, 0, 0, 0, 0, if down { 5 } else { 6 }, false)
            .is_ok()
        {
            guard.pen_button = down;
        }
    }
}

/// The second stylus button: same rules as the primary one.
fn secondary_button(profile: &dyn profiles::StylusProfile, down: bool, guard: &mut InjectDevices) {
    if down {
        if let Some(action) = profile.map(&StylusEvent::ButtonPress(StylusButton::Secondary)) {
            info!("Stylus button_secondary -> {}", action);
            guard.pen_button2_consumed = true;
            guard.run_action(&action);
            return;
        }
    } else if guard.pen_button2_consumed {
        guard.pen_button2_consumed = false;
        return;
    }
    if let Some(ref mut dev) = guard.pen {
        if dev
            .inject_pen(0, 0, 0, 0, 0, if down { 5 } else { 6 }, false)
            .is_ok()
        {
            guard.pen_button = down;
        }
    }
}

pub(super) fn handle_event(
    instance: u32,
    event: InputEvent,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
    settings_tx: &Option<watch::Sender<EncoderSettings>>,
    latency: &crate::latency::LatencyTracker,
    ping_notify: &Arc<tokio::sync::Notify>,
) {
    // Any input from the tablet counts as use for the idle-dim timer.
    if matches!(
        event,
        InputEvent::Touch { .. }
            | InputEvent::Pen { .. }
            | InputEvent::Rel { .. }
            | InputEvent::Button { .. }
            | InputEvent::Scroll { .. }
    ) {
        crate::options::note_activity(instance);
    }
    match event {
        InputEvent::Touch {
            x,
            y,
            pressure,
            action,
            slot,
        } => {
            // Stylus mode is a graphics tablet: fingers do nothing. The tablet app already keeps
            // them off the socket; this covers an older app and a mode switched mid-gesture.
            if crate::options::current_mode(instance) == crate::config::InputMode::Stylus {
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
        InputEvent::Pen {
            action: a @ (5 | 6),
            ..
        } => {
            note_pen_action(a);
            let profile = active_profile(instance);
            if let Ok(mut guard) = uinput.lock() {
                primary_button(profile.as_ref(), a == 5, &mut guard);
            }
        }
        InputEvent::Pen {
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
            eraser,
            action,
        } => {
            let abs_x = (x * COORD_MAX as f64) as i32;
            let abs_y = (y * COORD_MAX as f64) as i32;
            note_pen_action(action);
            let abs_pressure = (pressure * 4096.0) as i32;
            // Already degrees, as the tablet computes them. This used to
            // multiply by 180/π on the assumption they were radians, which
            // squashed a pen laid flat at 90° down to 57°.
            let tilt_x_deg = (tilt_x.round() as i32).clamp(-90, 90);
            let tilt_y_deg = (tilt_y.round() as i32).clamp(-90, 90);

            if let Ok(mut guard) = uinput.lock() {
                let ok = if let Some(ref mut dev) = guard.pen {
                    match dev.inject_pen(
                        abs_x,
                        abs_y,
                        abs_pressure,
                        tilt_x_deg,
                        tilt_y_deg,
                        action,
                        eraser,
                    ) {
                        Ok(_) => true,
                        Err(e) => {
                            warn!("Failed to inject pen: {}", e);
                            false
                        }
                    }
                } else {
                    match action {
                        0 => info!(
                            "Pen DOWN at ({}, {}), eraser={}, tilt=({:.1},{:.1})",
                            abs_x, abs_y, eraser, tilt_x, tilt_y
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
                    // Leaving proximity hides the tablet cursor, so hand the
                    // position to the plain pointer and let an ordinary cursor
                    // stay where the pen last was.
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
        }
        InputEvent::Resolution {
            width,
            height,
            width_mm,
            height_mm,
            preferred_transport,
            version,
        } => {
            info!(
                "Tablet reports native resolution: {}x{} ({}x{} mm), version: {:?}, transport: {:?}",
                width, height, width_mm, height_mm, version, preferred_transport
            );
            if let Some(ref pref) = preferred_transport {
                let mut cfg = crate::config::FileConfig::load();
                let mut changed = false;
                for p in cfg.tablets.values_mut() {
                    if p.preferred_transport.as_deref() != Some(pref) {
                        p.preferred_transport = Some(pref.clone());
                        changed = true;
                    }
                }
                if changed {
                    let _ = cfg.save();
                }
            }
            let Some(tx) = settings_tx else { return };
            if !crate::config::FileConfig::load().auto_resolution {
                info!("auto_resolution is off — keeping configured resolution");
                return;
            }
            if !(640..=8192).contains(&width) || !(480..=8192).contains(&height) {
                warn!("Ignoring implausible resolution {}x{}", width, height);
                return;
            }
            let mut new = tx.borrow().clone();
            // Reject nonsense physical sizes rather than baking them into an
            // EDID: a bad DPI makes the desktop come up at a absurd scale.
            let (mm_w, mm_h) =
                if (50..=1000).contains(&width_mm) && (50..=1000).contains(&height_mm) {
                    (width_mm, height_mm)
                } else {
                    (
                        crate::display::edid::DEFAULT_WIDTH_MM,
                        crate::display::edid::DEFAULT_HEIGHT_MM,
                    )
                };
            if new.width != width
                || new.height != height
                || new.width_mm != mm_w
                || new.height_mm != mm_h
            {
                new.width = width;
                new.height = height;
                new.width_mm = mm_w;
                new.height_mm = mm_h;
                info!(
                    "Auto-resolution: switching virtual display to {}x{} ({}x{} mm)",
                    width, height, mm_w, mm_h
                );
                let _ = tx.send(new);
            }
        }
        InputEvent::Rendered { seq, decode_us } => {
            latency.on_rendered(seq, decode_us);
        }
        InputEvent::Config {
            bitrate,
            fps,
            encoder,
        } => {
            let Some(tx) = settings_tx else {
                warn!("Received config from tablet but live settings are disabled");
                return;
            };
            let mut new = tx.borrow().clone();
            if let Some(b) = bitrate {
                // Clamped to the same ceiling the config file uses: an
                // unclamped value here would be persisted and poison every
                // later run, which is exactly how installs ended up pinned at
                // 200 Mbps with seconds of queueing delay.
                new.bitrate = b.clamp(
                    crate::config::MIN_BITRATE_KBPS,
                    crate::config::MAX_BITRATE_KBPS,
                );
                if new.bitrate != b {
                    warn!("Tablet asked for {} kbps — clamped to {}", b, new.bitrate);
                }
            }
            if let Some(f) = fps {
                new.fps = f.clamp(crate::config::MIN_FPS, crate::config::MAX_FPS);
            }
            if let Some(e) = encoder {
                new.encoder = e;
            }
            if *tx.borrow() != new {
                info!(
                    "Tablet pushed settings: encoder={} {}kbps @{}fps",
                    new.encoder, new.bitrate, new.fps
                );
                let _ = tx.send(new);
            }
        }

        // Already consumed by handle_connection; a second one is harmless.
        InputEvent::Auth { .. } => {}

        InputEvent::Rel { dx, dy } => {
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
        InputEvent::Button { button, down } => {
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
        InputEvent::Scroll { dx, dy } => {
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

        InputEvent::Mode { pen_only } => {
            // Older apps ask for graphics-tablet mode with this message. It is now the device's
            // input mode, stored like any other setting.
            let mode = if pen_only { "stylus" } else { "display" };
            info!("Tablet asked for {} mode", mode);
            crate::options::set_from_device(instance, "input_mode", &serde_json::json!(mode));
        }

        InputEvent::Transform { zoom, rotation } => {
            if let Ok(mut guard) = uinput.lock() {
                guard.transform(zoom, rotation);
            }
        }
        InputEvent::TransformEnd => {
            if let Ok(mut guard) = uinput.lock() {
                guard.transform_end();
            }
        }

        InputEvent::Gesture { kind, amount } => {
            if let Ok(mut guard) = uinput.lock() {
                match kind.as_str() {
                    // Ctrl + wheel zooms in most applications; fingers apart zoom in.
                    "pinch" => guard.scroll_with_modifier(
                        profiles::Modifier::Ctrl.code(),
                        (amount * PINCH_UNITS_PER_PX) as i32,
                    ),
                    // Alt + wheel; clockwise turns the wheel back.
                    "rotate" => guard.scroll_with_modifier(
                        profiles::Modifier::Alt.code(),
                        (-amount * ROTATE_UNITS_PER_PX) as i32,
                    ),
                    "workspace_prev" => guard.run_action(&HostAction::KeyPress {
                        mods: vec![profiles::Modifier::Meta],
                        key: "pageup",
                    }),
                    "workspace_next" => guard.run_action(&HostAction::KeyPress {
                        mods: vec![profiles::Modifier::Meta],
                        key: "pagedown",
                    }),
                    other => warn!("Unknown gesture {:?}", other),
                }
            }
        }

        InputEvent::StylusEvent {
            event,
            pressed,
            dx,
            dy,
            radians,
        } => {
            let profile = active_profile(instance);
            if let Ok(mut guard) = uinput.lock() {
                match event.as_str() {
                    "button_secondary" => secondary_button(profile.as_ref(), pressed, &mut guard),
                    "air_button" => {
                        let evs = guard.stylus.air_button(pressed);
                        dispatch_stylus(&mut guard, evs, profile.as_ref());
                    }
                    "air_motion" => {
                        let evs = guard.stylus.air_motion(dx, dy);
                        dispatch_stylus(&mut guard, evs, profile.as_ref());
                    }
                    "pen_gone" => guard.stylus.pen_gone(),
                    "orientation" => {
                        let evs = guard.stylus.orientation(radians);
                        dispatch_stylus(&mut guard, evs, profile.as_ref());
                    }
                    other => warn!("Unknown stylus event {:?}", other),
                }
            }
        }

        InputEvent::StylusTest { action } => {
            let reply = match action.parse::<HostAction>() {
                Ok(a) => {
                    info!("Stylus test: {}", a);
                    if let Ok(mut guard) = uinput.lock() {
                        guard.run_action(&a);
                    }
                    serde_json::json!({"type":"stylus_result","op":"test","ok":true,"message":a.to_string()})
                }
                Err(e) => {
                    serde_json::json!({"type":"stylus_result","op":"test","ok":false,"message":e})
                }
            };
            crate::net::notices::send(instance, reply);
        }
        InputEvent::StylusProfileSave { profile } => {
            let reply = match profiles::save(&profile) {
                Ok(id) => {
                    serde_json::json!({"type":"stylus_result","op":"save","ok":true,"message":id})
                }
                Err(e) => {
                    serde_json::json!({"type":"stylus_result","op":"save","ok":false,"message":e})
                }
            };
            crate::net::notices::send(instance, reply);
        }
        InputEvent::StylusProfileDelete { id } => {
            let reply = match profiles::delete(&id) {
                Ok(()) => {
                    serde_json::json!({"type":"stylus_result","op":"delete","ok":true,"message":id})
                }
                Err(e) => {
                    serde_json::json!({"type":"stylus_result","op":"delete","ok":false,"message":e})
                }
            };
            crate::net::notices::send(instance, reply);
        }

        InputEvent::Wake => {
            // The user touched the tablet (the app swallowed that touch to wake the screen, or a
            // finger came down on the touchpad). Count it as use and, in touchpad mode, wake the
            // host cursor: Mutter hides an idle pointer, and a 1 px out-and-back brings it back.
            crate::options::note_activity(instance);
            if crate::options::current_mode(instance) == crate::config::InputMode::Touchpad {
                if let Ok(mut guard) = uinput.lock() {
                    if let Some(dev) = guard.mouse() {
                        let _ = dev.move_by(1, 0);
                        let _ = dev.move_by(-1, 0);
                    }
                }
            }
        }

        InputEvent::Setting { key, value } => {
            crate::options::set_from_device(instance, &key, &value);
        }

        InputEvent::Ping => {
            info!("Received foreground ping from client — immediately re-asserting adb reverse tunnels");
            ping_notify.notify_waiters();
        }

        InputEvent::PreferredTransport { transport } => {
            info!("Client reported preferred transport switch: {}", transport);
            let mut cfg = crate::config::FileConfig::load();
            let mut changed = false;
            for p in cfg.tablets.values_mut() {
                if p.preferred_transport.as_deref() != Some(&transport) {
                    p.preferred_transport = Some(transport.clone());
                    changed = true;
                }
            }
            if changed {
                let _ = cfg.save();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The event frames (split at SYN_REPORT) a device wrote into `path`.
    fn frames(path: &std::path::Path) -> Vec<Vec<(u16, u16, i32)>> {
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

    fn device_over_file(name: &str) -> (UInputDevice, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("uscreen-{}-{}", name, std::process::id()));
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        (UInputDevice { file }, path)
    }

    #[test]
    fn pen_lift_sends_its_position_before_leaving_proximity() {
        let (mut dev, path) = device_over_file("penlift");
        dev.inject_pen(100, 200, 900, 5, -5, 0, false).unwrap(); // down: 2 frames
        dev.inject_pen(150, 250, 900, 5, -5, 2, false).unwrap(); // move
        dev.inject_pen(111, 222, 0, 7, -7, 1, false).unwrap(); // up
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.len(), 5, "down is two frames, move one, up two");
        let tip_up = &f[3];
        assert!(tip_up.contains(&(EV_ABS, ABS_X, 111)) && tip_up.contains(&(EV_ABS, ABS_Y, 222)));
        assert!(tip_up.contains(&(EV_ABS, ABS_TILT_X, 7)));
        assert!(tip_up.contains(&(EV_KEY, BTN_TOUCH, 0)));
        assert!(
            !tip_up.contains(&(EV_KEY, BTN_TOOL_PEN, 0)),
            "the tool must still be in proximity while the last sample goes out"
        );
        assert_eq!(
            f[4],
            vec![(EV_KEY, BTN_TOOL_PEN, 0)],
            "proximity out in its own frame"
        );
    }

    #[test]
    fn the_greeting_carries_the_computers_stream_settings_when_it_has_them() {
        let with = InputResponse {
            status: "connected".into(),
            width: 2800,
            height: 1752,
            codec: "h264".into(),
            pen_only: false,
            fps: Some(90),
            bitrate_kbps: Some(30000),
            host_version: None,
        };
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
        assert_eq!(v["fps"], 90);
        assert_eq!(v["bitrate_kbps"], 30000);
        // An older tablet app, or a session without settings, sees no new keys.
        let without = InputResponse {
            fps: None,
            bitrate_kbps: None,
            ..with
        };
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&without).unwrap()).unwrap();
        assert!(v.get("fps").is_none() && v.get("bitrate_kbps").is_none());
    }

    #[test]
    fn a_touching_pen_never_reports_zero_pressure() {
        let (mut dev, path) = device_over_file("lightpress");
        dev.inject_pen(100, 200, 0, 0, 0, 0, false).unwrap(); // light tap down, pressure 0
        dev.inject_pen(101, 201, 0, 0, 0, 2, false).unwrap(); // contact move, pressure 0
        dev.inject_pen(102, 202, 9000, 0, 0, 2, false).unwrap(); // above the axis maximum
        dev.inject_pen(103, 203, 0, 0, 0, 3, false).unwrap(); // hover
        dev.inject_pen(103, 203, 0, 0, 0, 1, false).unwrap(); // lift
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        let pressure = |frame: &Vec<(u16, u16, i32)>| {
            frame
                .iter()
                .find(|e| e.0 == EV_ABS && e.1 == ABS_PRESSURE)
                .map(|e| e.2)
        };
        assert_eq!(pressure(&f[1]), Some(1), "tip-down frame");
        assert_eq!(pressure(&f[2]), Some(1), "contact move");
        assert_eq!(
            pressure(&f[3]),
            Some(PEN_PRESSURE_MAX),
            "clamped to the axis range"
        );
        assert_eq!(pressure(&f[4]), Some(0), "hover stays at zero");
        assert_eq!(pressure(&f[5]), Some(0), "the lift stays at zero");
    }

    #[test]
    fn touchpad_motion_and_clicks_become_mouse_events() {
        let (mut dev, path) = device_over_file("mouse");
        dev.move_by(5, -3).unwrap();
        dev.move_by(0, 0).unwrap(); // nothing to say, no frame
        dev.press(0, true).unwrap();
        dev.press(0, false).unwrap();
        dev.press(1, true).unwrap();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.len(), 4);
        assert_eq!(f[0], vec![(EV_REL, REL_X, 5), (EV_REL, REL_Y, -3)]);
        assert_eq!(f[1], vec![(EV_KEY, BTN_LEFT, 1)]);
        assert_eq!(f[2], vec![(EV_KEY, BTN_LEFT, 0)]);
        assert_eq!(f[3], vec![(EV_KEY, BTN_RIGHT, 1)]);
    }

    #[test]
    fn scroll_sends_hi_res_units_and_whole_notches_from_the_remainder() {
        let (mut dev, path) = device_over_file("scroll");
        let mut rest = (0, 0);
        dev.scroll_by(0, 90, &mut rest).unwrap(); // not a notch yet
        dev.scroll_by(0, 90, &mut rest).unwrap(); // 180 total: one notch, 60 left
        dev.scroll_by(-240, 0, &mut rest).unwrap(); // two notches left
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(f[0], vec![(EV_REL, REL_WHEEL_HI_RES, 90)]);
        assert_eq!(
            f[1],
            vec![(EV_REL, REL_WHEEL_HI_RES, 90), (EV_REL, REL_WHEEL, 1)]
        );
        assert_eq!(
            f[2],
            vec![(EV_REL, REL_HWHEEL_HI_RES, -240), (EV_REL, REL_HWHEEL, -2)]
        );
        assert_eq!(rest, (0, 60));
    }

    #[test]
    fn touchpad_messages_parse() {
        let rel: InputEvent = serde_json::from_str(r#"{"type":"rel","dx":1.5,"dy":-2}"#).unwrap();
        assert!(matches!(rel, InputEvent::Rel { dx, dy } if dx == 1.5 && dy == -2.0));
        let b: InputEvent =
            serde_json::from_str(r#"{"type":"button","button":1,"down":true}"#).unwrap();
        assert!(matches!(
            b,
            InputEvent::Button {
                button: 1,
                down: true
            }
        ));
        let sc: InputEvent = serde_json::from_str(r#"{"type":"scroll","dx":0,"dy":12}"#).unwrap();
        assert!(matches!(sc, InputEvent::Scroll { dy, .. } if dy == 12.0));
    }

    #[test]
    fn key_combinations_press_modifiers_first_and_release_in_reverse() {
        let (mut dev, path) = device_over_file("keys");
        let codes = "ctrl+shift+z"
            .parse::<crate::stylus_profile::HostAction>()
            .unwrap()
            .key_codes();
        dev.tap_keys(&codes).unwrap();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            f,
            vec![
                vec![(EV_KEY, 29, 1), (EV_KEY, 42, 1), (EV_KEY, 44, 1)],
                vec![(EV_KEY, 44, 0), (EV_KEY, 42, 0), (EV_KEY, 29, 0)],
            ]
        );
    }

    fn custom(mappings: serde_json::Value) -> profiles::CustomProfile {
        profiles::CustomProfile::from_json(
            "t",
            &serde_json::json!({"id":"t","name":"T","mappings":mappings}),
        )
        .unwrap()
    }

    /// Devices backed by files, so what a stylus event turns into can be read back as evdev frames.
    fn devices_over_files(name: &str) -> (InjectDevices, std::path::PathBuf, std::path::PathBuf) {
        let (kb, kb_path) = device_over_file(&format!("{name}-kb"));
        let (pen, pen_path) = device_over_file(&format!("{name}-pen"));
        let mut d = InjectDevices::headless();
        d.keyboard = Some(kb);
        d.pen = Some(pen);
        (d, kb_path, pen_path)
    }

    #[test]
    fn a_custom_profile_turns_the_side_button_into_ctrl_c_and_swallows_the_stylus_button() {
        let (mut d, kb, pen) = devices_over_files("mapped");
        let profile = custom(serde_json::json!({"button_primary":"ctrl+c"}));
        primary_button(&profile, true, &mut d);
        primary_button(&profile, false, &mut d);
        let (keys, pen_frames) = (frames(&kb), frames(&pen));
        let _ = (std::fs::remove_file(&kb), std::fs::remove_file(&pen));
        assert_eq!(
            keys,
            vec![
                vec![(EV_KEY, 29, 1), (EV_KEY, 46, 1)],
                vec![(EV_KEY, 46, 0), (EV_KEY, 29, 0)],
            ]
        );
        assert!(
            pen_frames.is_empty(),
            "a mapped button must not also reach applications as BTN_STYLUS"
        );
        assert!(!d.pen_button_consumed, "the release clears the flag");
    }

    #[test]
    fn an_unmapped_side_button_still_reaches_applications_as_btn_stylus() {
        let (mut d, kb, pen) = devices_over_files("passthrough");
        for profile in [
            &profiles::StandardProfile as &dyn profiles::StylusProfile,
            &custom(serde_json::json!({"roll_cw":"ctrl+z"})),
        ] {
            primary_button(profile, true, &mut d);
            primary_button(profile, false, &mut d);
        }
        let (keys, pen_frames) = (frames(&kb), frames(&pen));
        let _ = (std::fs::remove_file(&kb), std::fs::remove_file(&pen));
        assert!(keys.is_empty());
        assert_eq!(
            pen_frames,
            vec![
                vec![(EV_KEY, BTN_STYLUS, 1)],
                vec![(EV_KEY, BTN_STYLUS, 0)],
                vec![(EV_KEY, BTN_STYLUS, 1)],
                vec![(EV_KEY, BTN_STYLUS, 0)],
            ]
        );
    }

    #[test]
    fn the_samsung_profile_turns_an_air_swipe_into_an_arrow_key() {
        let (mut d, kb, pen) = devices_over_files("air");
        let profile = profiles::SamsungSPenProfile;
        let mut events = d.stylus.air_button(true);
        events.extend(d.stylus.air_motion(-20.0, 1.0));
        events.extend(d.stylus.air_motion(-20.0, 1.0));
        events.extend(d.stylus.air_button(false));
        dispatch_stylus(&mut d, events, &profile);
        let keys = frames(&kb);
        let _ = (std::fs::remove_file(&kb), std::fs::remove_file(&pen));
        assert_eq!(
            keys,
            vec![vec![(EV_KEY, 105, 1)], vec![(EV_KEY, 105, 0)]],
            "one swipe left, one KEY_LEFT tap"
        );
    }

    #[test]
    fn a_pinch_is_two_fingers_on_the_gesture_pad_that_move_apart() {
        let (pad, path) = device_over_file("pinch");
        let mut d = InjectDevices::headless();
        d.gestures = Some(pad);
        d.transform(1.0, 0.0); // begin (both fingers down) + a no-change move
        d.transform(1.5, 0.0); // fingers 50% further apart
        d.transform_end();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        // begin, move, move, end
        assert_eq!(f.len(), 4);
        let has = |frame: &Vec<(u16, u16, i32)>, ev: (u16, u16, i32)| frame.contains(&ev);
        assert!(has(&f[0], (EV_KEY, BTN_TOOL_DOUBLETAP, 1)) && has(&f[0], (EV_KEY, BTN_TOUCH, 1)));
        // Slot 0 then slot 1 positions: 600 apart at the start, 900 after the 1.5x step.
        let xs = |frame: &Vec<(u16, u16, i32)>| -> Vec<i32> {
            frame
                .iter()
                .filter(|e| e.1 == ABS_MT_POSITION_X)
                .map(|e| e.2)
                .collect()
        };
        assert_eq!(xs(&f[0]), vec![700, 1300]);
        assert_eq!(xs(&f[2]), vec![550, 1450]);
        assert!(
            has(&f[3], (EV_KEY, BTN_TOOL_DOUBLETAP, 0))
                && has(&f[3], (EV_ABS, ABS_MT_TRACKING_ID, -1))
        );
        assert!(d.pinch.is_none());
    }

    #[test]
    fn rotating_turns_the_virtual_fingers_clockwise_on_screen() {
        let (pad, path) = device_over_file("rotate");
        let mut d = InjectDevices::headless();
        d.gestures = Some(pad);
        d.transform(1.0, 0.0);
        d.transform(1.0, 90.0);
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        let ys: Vec<i32> = f[2]
            .iter()
            .filter(|e| e.1 == ABS_MT_POSITION_Y)
            .map(|e| e.2)
            .collect();
        // A quarter turn clockwise (y grows downwards): the left finger goes up, the right one down.
        assert_eq!(ys, vec![300, 900]);
    }

    #[test]
    fn a_pinch_that_runs_out_of_pad_restarts_instead_of_leaving_it() {
        let (pad, path) = device_over_file("restart");
        let mut d = InjectDevices::headless();
        d.gestures = Some(pad);
        for _ in 0..6 {
            d.transform(1.2, 0.0);
        }
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        let max_x = f
            .iter()
            .flatten()
            .filter(|e| e.1 == ABS_MT_POSITION_X)
            .map(|e| e.2)
            .max()
            .unwrap();
        assert!(max_x < PAD_W, "fingers stay on the pad");
        let begins = f
            .iter()
            .filter(|fr| fr.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 1)))
            .count();
        assert!(
            begins >= 2,
            "a new set of fingers was put down when the range ran out"
        );
    }

    #[test]
    fn stylus_mode_has_no_relative_mouse_and_no_gesture_pad() {
        let (mouse, mpath) = device_over_file("stylus-mouse");
        let (pad, ppath) = device_over_file("stylus-pad");
        let mut d = InjectDevices::headless();
        d.mouse = Some(mouse);
        d.gestures = Some(pad);
        d.set_stylus_mode(true);
        assert!(
            d.mouse.is_none() && d.gestures.is_none(),
            "both devices are destroyed"
        );
        assert!(
            d.mouse().is_none(),
            "and not made again while in Stylus mode"
        );
        d.transform(1.3, 10.0);
        d.transform_end();
        let _ = (std::fs::remove_file(&mpath), std::fs::remove_file(&ppath));
        assert!(d.pinch.is_none() && d.gestures.is_none());
        d.set_stylus_mode(false);
        assert!(!d.stylus_mode);
    }

    #[test]
    fn entering_touchpad_mode_lifts_the_stylus_policy_and_keeps_existing_devices() {
        let (mouse, mpath) = device_over_file("ensure-mouse");
        let (pad, ppath) = device_over_file("ensure-pad");
        let (kb, kpath) = device_over_file("ensure-kb");
        let mut d = InjectDevices::headless();
        d.mouse = Some(mouse);
        d.gestures = Some(pad);
        d.keyboard = Some(kb);
        d.stylus_mode = true;
        d.ensure_devices_for(crate::config::InputMode::Touchpad);
        assert!(!d.stylus_mode, "Touchpad mode is not Stylus mode");
        assert!(d.mouse.is_some() && d.gestures.is_some() && d.keyboard.is_some());
        // Display mode needs none of them and leaves what exists alone.
        d.ensure_devices_for(crate::config::InputMode::Display);
        assert!(d.mouse.is_some());
        for p in [mpath, ppath, kpath] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn in_stylus_mode_a_profiles_click_goes_through_the_absolute_pointer() {
        let (pointer, path) = device_over_file("stylus-click");
        let mut d = InjectDevices::headless();
        d.pointer = Some(pointer);
        d.last_pen_pos = (1000, 2000);
        d.set_stylus_mode(true);
        d.run_action(&crate::stylus_profile::HostAction::MouseClick(
            crate::stylus_profile::MouseButton::Right,
        ));
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert!(d.mouse.is_none(), "no relative mouse was made for it");
        assert_eq!(
            f,
            vec![
                vec![(EV_ABS, ABS_X, 1000), (EV_ABS, ABS_Y, 2000)],
                vec![(EV_KEY, BTN_RIGHT, 1)],
                vec![(EV_KEY, BTN_RIGHT, 0)],
            ]
        );
    }

    #[test]
    fn stylus_and_gesture_messages_parse() {
        let g: InputEvent =
            serde_json::from_str(r#"{"type":"gesture","kind":"pinch","amount":-12.5}"#).unwrap();
        assert!(
            matches!(g, InputEvent::Gesture { ref kind, amount } if kind == "pinch" && amount == -12.5)
        );
        let e: InputEvent =
            serde_json::from_str(r#"{"type":"stylus_event","event":"air_motion","dx":3,"dy":-4}"#)
                .unwrap();
        assert!(
            matches!(e, InputEvent::StylusEvent { dx, dy, pressed: false, .. } if dx == 3.0 && dy == -4.0)
        );
        let tr: InputEvent =
            serde_json::from_str(r#"{"type":"transform","zoom":1.04,"rotation":-2.5}"#).unwrap();
        assert!(
            matches!(tr, InputEvent::Transform { zoom, rotation } if zoom == 1.04 && rotation == -2.5)
        );
        assert!(matches!(
            serde_json::from_str::<InputEvent>(r#"{"type":"transform_end"}"#).unwrap(),
            InputEvent::TransformEnd
        ));
        let t: InputEvent =
            serde_json::from_str(r#"{"type":"stylus_test","action":"ctrl+c"}"#).unwrap();
        assert!(matches!(t, InputEvent::StylusTest { ref action } if action == "ctrl+c"));
    }

    #[test]
    fn eraser_lift_leaves_proximity_with_the_eraser_tool() {
        let (mut dev, path) = device_over_file("eraserlift");
        dev.inject_pen(10, 20, 500, 0, 0, 0, true).unwrap();
        dev.inject_pen(10, 20, 0, 0, 0, 1, true).unwrap();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.last().unwrap(), &vec![(EV_KEY, BTN_TOOL_RUBBER, 0)]);
    }
}
