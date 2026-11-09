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

/// Wheel units per tablet pixel of pinch travel (Ctrl + wheel zooms): a 100 px pinch is about five
/// notches.
const PINCH_UNITS_PER_PX: f64 = 6.0;

/// Route one pen sample (from the Android pen or a physical tablet) according to the device's
/// options: `auto` is the unified pen (tablet data always, mouse clicks for non-art apps), `mouse` (Stylus mode) is the pen as a mouse, anything else is
/// the plain pressure pen.
pub(super) fn apply_pen(
    guard: &mut InjectDevices,
    opts: &crate::options::DeviceOptions,
    s: &PenSample,
) {
    let (abs_x, abs_y, action, eraser) = (s.x, s.y, s.action, s.eraser);
    // Ghost tracking: both devices follow the pen. Not on X11, where the tablet already drives the
    // one core pointer, and not without the absolute pointer.
    if opts.stylus_style == "auto" && guard.has_pointer() && !session_is_x11() {
        let to_pen = route_to_pen(&opts.stylus_art_apps);
        guard.auto_pen(s, to_pen, &opts.stylus_button_primary, &opts.stylus_button_secondary);
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

pub(super) fn handle_event(
    instance: u32,
    event: InputEvent,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
    settings_tx: &Option<watch::Sender<EncoderSettings>>,
    latency: &crate::latency::LatencyTracker,
    ping_notify: &Arc<tokio::sync::Notify>,
) {
    // A pinch whose end never arrived must not leave Ctrl held.
    if !matches!(event, InputEvent::Gesture { .. }) {
        if let Ok(mut guard) = uinput.lock() {
            guard.expire_pinch();
        }
    }
    // Any input from the tablet counts as use for the idle-dim timer.
    if matches!(
        event,
        InputEvent::Touch { .. }
            | InputEvent::Pen { .. }
            | InputEvent::Rel { .. }
            | InputEvent::Button { .. }
            | InputEvent::Scroll { .. }
            | InputEvent::TouchpadRaw { .. }
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

            let opts = crate::options::current_options(instance);
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

        InputEvent::TouchpadRaw { slots } => {
            // Only while native gestures are in use in Touchpad mode: an old or confused tablet
            // must not bring a touchpad into existence in Stylus or Display mode.
            let opts = crate::options::current_options(instance);
            if opts.input_mode != crate::config::InputMode::Touchpad
                || !opts.native_gestures_effective()
            {
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

        InputEvent::Wake => {
            // The user touched the tablet (the app swallowed that touch to wake the screen, or a
            // finger came down on the touchpad). Count it as use and, in touchpad mode, wake the
            // host cursor: Mutter hides an idle pointer, and a 1 px out-and-back brings it back.
            crate::options::note_activity(instance);
            // Not with native gestures: the virtual touchpad's own motion shows the cursor, and a
            // second device twitching beside it only confuses the compositor.
            let opts = crate::options::current_options(instance);
            if opts.input_mode == crate::config::InputMode::Touchpad
                && !opts.native_gestures_effective()
            {
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
    fn stylus_mouse_style_holds_left_from_pen_down_to_pen_up() {
        let (dev, path) = device_over_file("penmouse");
        let mut inj = InjectDevices::headless();
        inj.pointer = Some(dev);
        inj.pen_as_mouse(1000, 2000, 3, "right", "middle"); // hover
        inj.pen_as_mouse(1000, 2000, 0, "right", "middle"); // tip down
        for i in 1..=10 {
            inj.pen_as_mouse(1000 + i * 40, 2000, 2, "right", "middle");
        }
        inj.pen_as_mouse(1400, 2000, 1, "right", "middle"); // tip up
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        let left: Vec<(usize, i32)> = f
            .iter()
            .enumerate()
            .filter_map(|(i, fr)| {
                fr.iter()
                    .find(|e| e.0 == EV_KEY && e.1 == BTN_LEFT)
                    .map(|e| (i, e.2))
            })
            .collect();
        assert_eq!(left.len(), 2, "one press and one release, nothing in between: {left:?}");
        assert_eq!(left[0].1, 1);
        assert_eq!(left[1].1, 0);
        let moves_while_held = f[left[0].0..left[1].0]
            .iter()
            .filter(|fr| fr.iter().any(|e| e.0 == EV_ABS && e.1 == ABS_X))
            .count();
        assert!(moves_while_held >= 10, "{moves_while_held} position frames while the button was held");
        assert_eq!(inj.pen_mouse_buttons, 0);
    }

    #[test]
    fn stylus_side_buttons_follow_the_configured_mapping() {
        let (dev, path) = device_over_file("penbuttons");
        let mut inj = InjectDevices::headless();
        inj.pointer = Some(dev);
        inj.pen_as_mouse(0, 0, 5, "right", "middle");
        inj.pen_as_mouse(0, 0, 6, "right", "middle");
        inj.pen_as_mouse(0, 0, 7, "right", "middle");
        inj.pen_as_mouse(0, 0, 8, "right", "middle");
        inj.pen_as_mouse(0, 0, 5, "none", "middle"); // disabled: nothing
        inj.pen_as_mouse(0, 0, 7, "right", "right"); // swapped
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        let keys: Vec<(u16, i32)> = f
            .iter()
            .flat_map(|fr| fr.iter().filter(|e| e.0 == EV_KEY).map(|e| (e.1, e.2)))
            .collect();
        assert_eq!(
            keys,
            vec![
                (BTN_RIGHT, 1),
                (BTN_RIGHT, 0),
                (BTN_MIDDLE, 1),
                (BTN_MIDDLE, 0),
                (BTN_RIGHT, 1)
            ]
        );
        inj.release_pen_mouse();
        assert_eq!(inj.pen_mouse_buttons, 0);
    }

    #[test]
    fn pinch_holds_ctrl_for_every_step_and_lets_go_at_the_end() {
        let (kb, kpath) = device_over_file("pinchkb");
        let (mouse, mpath) = device_over_file("pinchmouse");
        let mut inj = InjectDevices::headless();
        inj.keyboard = Some(kb);
        inj.mouse = Some(mouse);
        inj.pinch_step(120);
        inj.pinch_step(120);
        inj.pinch_step(-120);
        inj.end_pinch();
        let k = frames(&kpath);
        let m = frames(&mpath);
        let _ = std::fs::remove_file(&kpath);
        let _ = std::fs::remove_file(&mpath);
        assert_eq!(
            k,
            vec![
                vec![(EV_KEY, KEY_LEFTCTRL, 1)],
                vec![(EV_KEY, KEY_LEFTCTRL, 0)]
            ],
            "Ctrl goes down once and comes up once"
        );
        assert_eq!(m.len(), 3, "three wheel steps");
        assert!(inj.pinch_since.is_none());
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
        d.ensure_devices_for(crate::config::InputMode::Touchpad, false);
        assert!(!d.stylus_mode, "Touchpad mode is not Stylus mode");
        assert!(d.mouse.is_some() && d.keyboard.is_some());
        // Display mode needs none of them and leaves what exists alone.
        d.ensure_devices_for(crate::config::InputMode::Display, false);
        assert!(d.mouse.is_some());
        for p in [mpath, kpath] {
            let _ = std::fs::remove_file(p);
        }
    }

    fn finger(id: i32, x: f64, y: f64) -> PadFinger {
        PadFinger {
            id,
            x_mm: x,
            y_mm: y,
            pressure: 0.5,
        }
    }

    #[test]
    fn touchpad_raw_message_parses() {
        let m: InputEvent = serde_json::from_str(
            r#"{"type":"touchpad_raw","slots":[{"id":3,"x":120.5,"y":35,"pressure":0.4},{"id":4,"x":1,"y":2}]}"#,
        )
        .unwrap();
        match m {
            InputEvent::TouchpadRaw { slots } => {
                assert_eq!(slots.len(), 2);
                assert_eq!((slots[0].id, slots[0].x, slots[0].y), (3, 120.5, 35.0));
                assert_eq!(slots[1].pressure, 0.0, "pressure is optional");
            }
            other => panic!("wrong message {other:?}"),
        }
        let empty: InputEvent = serde_json::from_str(r#"{"type":"touchpad_raw","slots":[]}"#).unwrap();
        assert!(matches!(empty, InputEvent::TouchpadRaw { slots } if slots.is_empty()));
    }

    #[test]
    fn pad_frames_assign_slots_and_tracking_ids_and_lift_with_minus_one() {
        let (mut dev, path) = device_over_file("pad");
        let mut st = PadState::new();
        // Two fingers land together.
        dev.inject_pad(&mut st, &[finger(7, 10.0, 20.0), finger(9, 30.0, 40.0)])
            .unwrap();
        // Both move; the first one is the same slot as before.
        dev.inject_pad(&mut st, &[finger(7, 11.0, 20.0), finger(9, 31.0, 40.0)])
            .unwrap();
        // The first lifts, the second stays.
        dev.inject_pad(&mut st, &[finger(9, 31.0, 41.0)]).unwrap();
        // Everything lifts; then nothing more is written for an empty list.
        dev.inject_pad(&mut st, &[]).unwrap();
        dev.inject_pad(&mut st, &[]).unwrap();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(f.len(), 4, "an empty frame on an empty pad writes nothing");

        let down = &f[0];
        assert!(down.contains(&(EV_ABS, ABS_MT_SLOT, 0)) && down.contains(&(EV_ABS, ABS_MT_SLOT, 1)));
        assert!(down.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 0)));
        assert!(down.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 1)));
        assert!(down.contains(&(EV_ABS, ABS_MT_POSITION_X, 400)), "10 mm = 400 units");
        assert!(down.contains(&(EV_ABS, ABS_MT_POSITION_Y, 800)));
        assert!(down.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 1)) && down.contains(&(EV_KEY, BTN_TOUCH, 1)));

        let moved = &f[1];
        assert!(
            !moved.iter().any(|e| e.1 == ABS_MT_TRACKING_ID),
            "tracking ids are only sent when a finger lands"
        );
        assert!(moved.contains(&(EV_ABS, ABS_MT_POSITION_X, 440)));

        let one_left = &f[2];
        assert!(one_left.contains(&(EV_ABS, ABS_MT_SLOT, 0)));
        assert!(one_left.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)), "slot 0 lifted");
        assert!(one_left.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 0)));
        assert!(one_left.contains(&(EV_KEY, BTN_TOOL_FINGER, 1)));

        let none = &f[3];
        assert!(none.contains(&(EV_ABS, ABS_MT_SLOT, 1)) && none.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)));
        assert!(none.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)) && none.contains(&(EV_KEY, BTN_TOUCH, 0)));
        assert_eq!(st.count(), 0);
    }

    #[test]
    fn pad_reuses_a_freed_slot_with_a_fresh_tracking_id_and_clamps_to_the_surface() {
        let (mut dev, path) = device_over_file("padreuse");
        let mut st = PadState::new();
        dev.inject_pad(&mut st, &[finger(1, 5.0, 5.0)]).unwrap();
        dev.inject_pad(&mut st, &[]).unwrap();
        dev.inject_pad(&mut st, &[finger(2, 9999.0, -50.0)]).unwrap();
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        let again = &f[2];
        assert!(again.contains(&(EV_ABS, ABS_MT_SLOT, 0)), "the lowest free slot");
        assert!(again.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 1)), "never reuses a tracking id");
        assert!(again.contains(&(EV_ABS, ABS_MT_POSITION_X, 640 * 40 - 1)), "clamped to the pad");
        assert!(again.contains(&(EV_ABS, ABS_MT_POSITION_Y, 0)));
    }

    #[test]
    fn pad_finger_counts_pick_the_tool_button() {
        let (mut dev, path) = device_over_file("padtools");
        let mut st = PadState::new();
        let hand: Vec<PadFinger> = (0..5).map(|i| finger(i, 10.0 * i as f64, 10.0)).collect();
        for n in [3, 4, 5] {
            dev.inject_pad(&mut st, &hand[..n]).unwrap();
        }
        let f = frames(&path);
        let _ = std::fs::remove_file(&path);
        assert!(f[0].contains(&(EV_KEY, BTN_TOOL_TRIPLETAP, 1)));
        assert!(f[1].contains(&(EV_KEY, BTN_TOOL_TRIPLETAP, 0)) && f[1].contains(&(EV_KEY, BTN_TOOL_QUADTAP, 1)));
        assert!(f[2].contains(&(EV_KEY, BTN_TOOL_QUINTTAP, 1)));
    }

    #[test]
    fn leaving_touchpad_mode_removes_the_pad_and_lifts_its_fingers() {
        let (pad, ppath) = device_over_file("leavepad");
        let mut d = InjectDevices::headless();
        d.pad = Some(pad);
        d.pad_state.slots[0] = Some(1);
        d.pad_state.tool = Some(BTN_TOOL_FINGER);
        d.ensure_devices_for(crate::config::InputMode::Display, true);
        assert!(d.pad.is_none(), "Display mode has no touchpad");
        let f = frames(&ppath);
        let _ = std::fs::remove_file(&ppath);
        assert!(f[0].contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)), "the finger was lifted first");
        assert_eq!(d.pad_state.count(), 0);
    }

    #[test]
    fn native_gestures_need_wayland_gnome_or_kde() {
        use crate::options::native_gestures_supported as ok;
        assert!(ok("wayland", "ubuntu:GNOME"));
        assert!(ok("wayland", "GNOME"));
        assert!(ok("Wayland", "KDE"));
        assert!(!ok("x11", "GNOME"), "X11 never turns libinput gestures into actions");
        assert!(!ok("x11", "KDE"));
        assert!(!ok("wayland", "X-Cinnamon"));
        assert!(!ok("wayland", "sway"));
        assert!(!ok("", ""));
    }

    /// Plays gestures on the REAL virtual touchpad so `libinput debug-events` (and the desktop) can
    /// be watched reacting. Moves the real cursor and switches real workspaces, so it only runs when
    /// asked: `cargo test -p uscreen real_pad -- --ignored --nocapture`. `PAD_SCRIPT` picks the
    /// gestures (comma list of move,scroll,swipe3l,swipe3r,swipe3u,swipe3d,swipe4l,pinchout,pinchin,
    /// tap); default is all of them.
    #[test]
    #[ignore]
    fn real_pad_plays_gestures_for_libinput() {
        let script = std::env::var("PAD_SCRIPT").unwrap_or_else(|_| {
            "move,scroll,swipe3l,swipe3r,swipe3u,swipe3d,swipe4l,pinchout,pinchin,tap".into()
        });
        let mut d = InjectDevices::headless();
        assert!(d.pad().is_some(), "uinput must be available");
        // Let udev and the compositor discover the new device.
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let frame = std::time::Duration::from_millis(8);
        let pause = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
        // Fingers 0..n at (cx + spread*i, cy) travel by (dx, dy) over `steps` frames, then lift.
        let travel = |d: &mut InjectDevices, n: usize, cx: f64, cy: f64, spread: f64, dx: f64, dy: f64, steps: u32| {
            let at = |t: f64| -> Vec<PadFinger> {
                (0..n)
                    .map(|i| finger(i as i32 + 1, cx + spread * i as f64 + dx * t, cy + dy * t))
                    .collect()
            };
            d.touchpad_raw(&at(0.0));
            std::thread::sleep(frame * 4);
            for k in 1..=steps {
                d.touchpad_raw(&at(k as f64 / steps as f64));
                std::thread::sleep(frame);
            }
            std::thread::sleep(frame * 4);
            d.touchpad_raw(&[]);
        };
        // Two fingers `gap` apart (centred) that end `gap2` apart.
        let pinch = |d: &mut InjectDevices, gap: f64, gap2: f64, steps: u32| {
            let at = |t: f64| -> Vec<PadFinger> {
                let g = gap + (gap2 - gap) * t;
                vec![finger(1, 200.0 - g / 2.0, 120.0), finger(2, 200.0 + g / 2.0, 120.0)]
            };
            d.touchpad_raw(&at(0.0));
            std::thread::sleep(frame * 4);
            for k in 1..=steps {
                d.touchpad_raw(&at(k as f64 / steps as f64));
                std::thread::sleep(frame);
            }
            std::thread::sleep(frame * 4);
            d.touchpad_raw(&[]);
        };
        for step in script.split(',') {
            eprintln!("PAD-STEP {step}");
            match step.trim() {
                "move" => travel(&mut d, 1, 150.0, 120.0, 0.0, 40.0, 10.0, 40),
                "scroll" => travel(&mut d, 2, 150.0, 120.0, 25.0, 0.0, 40.0, 40),
                "swipe3l" => travel(&mut d, 3, 220.0, 120.0, 25.0, -90.0, 0.0, 60),
                "swipe3r" => travel(&mut d, 3, 130.0, 120.0, 25.0, 90.0, 0.0, 60),
                "swipe3u" => travel(&mut d, 3, 150.0, 170.0, 25.0, 0.0, -80.0, 60),
                "swipe3d" => travel(&mut d, 3, 150.0, 90.0, 25.0, 0.0, 80.0, 60),
                "swipe4l" => travel(&mut d, 4, 220.0, 120.0, 22.0, -90.0, 0.0, 60),
                "pinchout" => pinch(&mut d, 30.0, 110.0, 50),
                "pinchin" => pinch(&mut d, 110.0, 30.0, 50),
                "tap" => {
                    d.touchpad_raw(&[finger(1, 200.0, 120.0)]);
                    pause(60);
                    d.touchpad_raw(&[]);
                }
                // One finger at a steady 100 mm/s: a frame every 8 ms ...
                "glide_even" => {
                    d.touchpad_raw(&[finger(1, 100.0, 120.0)]);
                    pause(30);
                    for k in 1..=60 {
                        d.touchpad_raw(&[finger(1, 100.0 + 0.8 * k as f64, 120.0)]);
                        pause(8);
                    }
                    d.touchpad_raw(&[]);
                }
                // ... and the same speed arriving as two frames 2 ms apart every 16 ms.
                "glide_pairs" => {
                    d.touchpad_raw(&[finger(1, 100.0, 120.0)]);
                    pause(30);
                    for k in 1..=30 {
                        d.touchpad_raw(&[finger(1, 100.0 + 1.6 * k as f64 - 0.8, 120.0)]);
                        pause(2);
                        d.touchpad_raw(&[finger(1, 100.0 + 1.6 * k as f64, 120.0)]);
                        pause(14);
                    }
                    d.touchpad_raw(&[]);
                }
                other => panic!("unknown PAD_SCRIPT step {other}"),
            }
            pause(900);
        }
        pause(500);
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
