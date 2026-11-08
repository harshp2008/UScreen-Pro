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
                "Pen events: down={} up={} move={} hover={} hover_exit={} btn_down={} btn_up={} btn2={}",
                c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]
            );
            *c = [0; 8];
        }
    }
}

/// High-resolution wheel units per tablet pixel of two-finger travel: one notch (120 units) per
/// 40 px, about what a laptop touchpad feels like.
const SCROLL_UNITS_PER_PX: f64 = 3.0;

/// Wheel units per tablet pixel of pinch travel (Ctrl + wheel zooms): a 100 px pinch is about five
/// notches.
const PINCH_UNITS_PER_PX: f64 = 6.0;

/// What a pen button does, from its `stylus_button_*` setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PenButtonAction {
    /// Hold a mouse button (BTN_LEFT / RIGHT / MIDDLE) for as long as the pen button is down.
    Mouse(u16),
    /// Pass it on as the tablet's own stylus button (BTN_STYLUS).
    Stylus,
    None,
}

impl PenButtonAction {
    fn from_setting(setting: &str) -> Self {
        match setting {
            "left_click" => Self::Mouse(BTN_LEFT),
            "right_click" => Self::Mouse(BTN_RIGHT),
            "middle_click" => Self::Mouse(BTN_MIDDLE),
            "stylus_button" => Self::Stylus,
            _ => Self::None,
        }
    }
}

/// A pen button changed (pen actions 5 / 6 = side button down / up, 7 / 8 = second button). A mouse
/// action is held for as long as the pen button is, so it can drag; a release lets go of whatever
/// the press took, even if the setting changed in between.
fn pen_button(buttons: &(String, String), action: u8, guard: &mut InjectDevices) {
    let second = action >= 7;
    let down = action == 5 || action == 7;
    let slot = second as usize;
    if !down {
        if let Some(code) = guard.pen_mouse_held[slot].take() {
            guard.pointer_button(code, false);
            return;
        }
    }
    match PenButtonAction::from_setting(if second { &buttons.1 } else { &buttons.0 }) {
        PenButtonAction::Mouse(code) if down => {
            guard.pen_mouse_held[slot] = Some(code);
            guard.pointer_button(code, true);
        }
        PenButtonAction::Stylus => {
            if let Some(ref mut dev) = guard.pen {
                let wire = if down { 5 } else { 6 };
                if dev.inject_pen(0, 0, 0, 0, 0, wire, false).is_ok() {
                    guard.pen_button = down;
                }
            }
        }
        _ => {}
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
            action: action @ 5..=8,
            ..
        } => {
            note_pen_action(action);
            if let Ok(mut guard) = uinput.lock() {
                pen_button(
                    &crate::options::current_pen_buttons(instance),
                    action,
                    &mut guard,
                );
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
                    if action == 4 || (action == 3 && guard.stylus_mode) {
                        // The tablet cursor vanishes when the tool leaves proximity, and in Stylus
                        // mode the plain pointer also follows the hovering pen, so an ordinary
                        // mouse cursor is always under it (S-Pen-Mouse does the same).
                        guard.park_pointer();
                    }
                    match action {
                        0 | 3 => guard.pen_proximity = true,
                        1 | 4 => guard.pen_proximity = false,
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

        InputEvent::Gesture { kind, amount } => {
            if let Ok(mut guard) = uinput.lock() {
                match kind.as_str() {
                    // Ctrl + wheel zooms in browsers and most viewers; fingers apart zoom in.
                    "pinch" => guard.pinch((amount * PINCH_UNITS_PER_PX) as i32),
                    "pinch_end" => guard.pinch_end(),
                    "workspace_prev" => guard.tap_key_combo(&[KEY_LEFTMETA, KEY_PAGEUP]),
                    "workspace_next" => guard.tap_key_combo(&[KEY_LEFTMETA, KEY_PAGEDOWN]),
                    other => warn!("Unknown gesture {:?}", other),
                }
            }
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
        dev.tap_keys(&[KEY_LEFTMETA, KEY_PAGEUP]).unwrap();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            f,
            vec![
                vec![(EV_KEY, KEY_LEFTMETA, 1), (EV_KEY, KEY_PAGEUP, 1)],
                vec![(EV_KEY, KEY_PAGEUP, 0), (EV_KEY, KEY_LEFTMETA, 0)],
            ]
        );
    }

    #[test]
    fn stylus_mode_has_no_relative_mouse() {
        let (mouse, mpath) = device_over_file("stylus-mouse");
        let mut d = InjectDevices::headless();
        d.mouse = Some(mouse);
        d.set_stylus_mode(true);
        assert!(d.mouse.is_none(), "the relative mouse is destroyed");
        assert!(
            d.mouse().is_none(),
            "and not made again while in Stylus mode"
        );
        let _ = std::fs::remove_file(&mpath);
        d.set_stylus_mode(false);
        assert!(!d.stylus_mode);
    }

    #[test]
    fn entering_touchpad_mode_lifts_the_stylus_policy_and_keeps_existing_devices() {
        let (mouse, mpath) = device_over_file("ensure-mouse");
        let (kb, kpath) = device_over_file("ensure-kb");
        let mut d = InjectDevices::headless();
        d.mouse = Some(mouse);
        d.keyboard = Some(kb);
        d.stylus_mode = true;
        d.ensure_devices_for(crate::config::InputMode::Touchpad);
        assert!(!d.stylus_mode, "Touchpad mode is not Stylus mode");
        assert!(d.mouse.is_some() && d.keyboard.is_some());
        // Display mode needs none of them and leaves what exists alone.
        d.ensure_devices_for(crate::config::InputMode::Display);
        assert!(d.mouse.is_some());
        for p in [mpath, kpath] {
            let _ = std::fs::remove_file(p);
        }
    }

    fn pen_buttons(primary: &str, secondary: &str) -> (String, String) {
        (primary.to_string(), secondary.to_string())
    }

    #[test]
    fn the_pen_side_button_is_a_held_middle_click_by_default_and_the_second_a_right_click() {
        let (pointer, path) = device_over_file("penbtn");
        let mut d = InjectDevices::headless();
        d.pointer = Some(pointer);
        d.last_pen_pos = (500, 700);
        let cfg = pen_buttons("middle_click", "right_click");
        pen_button(&cfg, 5, &mut d); // side button down
        pen_button(&cfg, 7, &mut d); // second button down while the first is held
        pen_button(&cfg, 6, &mut d);
        pen_button(&cfg, 8, &mut d);
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            f,
            vec![
                vec![(EV_ABS, ABS_X, 500), (EV_ABS, ABS_Y, 700)], // pointer parked under the pen
                vec![(EV_KEY, BTN_MIDDLE, 1)],
                vec![(EV_ABS, ABS_X, 500), (EV_ABS, ABS_Y, 700)],
                vec![(EV_KEY, BTN_RIGHT, 1)],
                vec![(EV_KEY, BTN_MIDDLE, 0)],
                vec![(EV_KEY, BTN_RIGHT, 0)],
            ]
        );
        assert_eq!(d.pen_mouse_held, [None, None]);
    }

    #[test]
    fn a_pen_button_release_lets_go_of_what_the_press_took_even_if_the_setting_changed() {
        let (pointer, path) = device_over_file("penbtn-change");
        let mut d = InjectDevices::headless();
        d.pointer = Some(pointer);
        pen_button(&pen_buttons("right_click", "none"), 5, &mut d);
        pen_button(&pen_buttons("left_click", "none"), 6, &mut d);
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            f.contains(&vec![(EV_KEY, BTN_RIGHT, 0)]),
            "released as the right button"
        );
        assert!(!f.iter().flatten().any(|e| e.1 == BTN_LEFT));
    }

    #[test]
    fn a_pen_button_can_be_passed_on_as_the_stylus_button_or_do_nothing() {
        let (pen, ppath) = device_over_file("penbtn-stylus");
        let (pointer, qpath) = device_over_file("penbtn-none");
        let mut d = InjectDevices::headless();
        d.pen = Some(pen);
        d.pointer = Some(pointer);
        pen_button(&pen_buttons("stylus_button", "none"), 5, &mut d);
        pen_button(&pen_buttons("stylus_button", "none"), 6, &mut d);
        pen_button(&pen_buttons("stylus_button", "none"), 7, &mut d);
        pen_button(&pen_buttons("stylus_button", "none"), 8, &mut d);
        let (pen_frames, pointer_frames) = (frames(&ppath), frames(&qpath));
        let _ = (std::fs::remove_file(&ppath), std::fs::remove_file(&qpath));
        assert_eq!(
            pen_frames,
            vec![vec![(EV_KEY, BTN_STYLUS, 1)], vec![(EV_KEY, BTN_STYLUS, 0)]]
        );
        assert!(pointer_frames.is_empty(), "'none' sends nothing");
    }

    #[test]
    fn a_dropped_connection_releases_a_held_pen_mouse_button() {
        let (pointer, path) = device_over_file("penbtn-drop");
        let mut d = InjectDevices::headless();
        d.pointer = Some(pointer);
        pen_button(&pen_buttons("middle_click", "none"), 5, &mut d);
        d.release_all();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.last().unwrap(), &vec![(EV_KEY, BTN_MIDDLE, 0)]);
    }

    #[test]
    fn a_pinch_holds_ctrl_for_the_whole_gesture_and_lets_go_at_the_end() {
        let (kb, kpath) = device_over_file("pinch-kb");
        let (mouse, mpath) = device_over_file("pinch-mouse");
        let mut d = InjectDevices::headless();
        d.keyboard = Some(kb);
        d.mouse = Some(mouse);
        d.pinch(240); // Ctrl goes down, then the wheel turns
        d.pinch(120);
        d.pinch(-60);
        let during = frames(&kpath);
        d.pinch_end();
        let after = frames(&kpath);
        let wheel = frames(&mpath);
        let _ = (std::fs::remove_file(&kpath), std::fs::remove_file(&mpath));
        assert_eq!(
            during,
            vec![vec![(EV_KEY, KEY_LEFTCTRL, 1)]],
            "one press for three wheel steps, no release in between"
        );
        assert_eq!(after.last().unwrap(), &vec![(EV_KEY, KEY_LEFTCTRL, 0)]);
        assert_eq!(wheel.len(), 3, "every step turned the wheel");
        assert!(d.pinch_ctrl_since.is_none());
    }

    #[test]
    fn a_pinch_that_never_ends_lets_go_of_ctrl_on_its_own() {
        let (kb, kpath) = device_over_file("pinch-stale-kb");
        let (mouse, mpath) = device_over_file("pinch-stale-mouse");
        let mut d = InjectDevices::headless();
        d.keyboard = Some(kb);
        d.mouse = Some(mouse);
        d.pinch(100);
        d.release_stale_pinch();
        assert!(d.pinch_ctrl_since.is_some(), "still within the hold time");
        d.pinch_ctrl_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(2));
        d.release_stale_pinch();
        let f = frames(&kpath);
        let _ = (std::fs::remove_file(&kpath), std::fs::remove_file(&mpath));
        assert_eq!(f.last().unwrap(), &vec![(EV_KEY, KEY_LEFTCTRL, 0)]);
        assert!(d.pinch_ctrl_since.is_none());
    }

    #[test]
    fn a_dropped_connection_releases_ctrl_held_for_a_pinch() {
        let (kb, kpath) = device_over_file("pinch-drop-kb");
        let (mouse, mpath) = device_over_file("pinch-drop-mouse");
        let mut d = InjectDevices::headless();
        d.keyboard = Some(kb);
        d.mouse = Some(mouse);
        d.pinch(100);
        d.release_all();
        let f = frames(&kpath);
        let _ = (std::fs::remove_file(&kpath), std::fs::remove_file(&mpath));
        assert_eq!(f.last().unwrap(), &vec![(EV_KEY, KEY_LEFTCTRL, 0)]);
    }

    #[test]
    fn gesture_messages_parse() {
        let g: InputEvent =
            serde_json::from_str(r#"{"type":"gesture","kind":"pinch","amount":-12.5}"#).unwrap();
        assert!(
            matches!(g, InputEvent::Gesture { ref kind, amount } if kind == "pinch" && amount == -12.5)
        );
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
