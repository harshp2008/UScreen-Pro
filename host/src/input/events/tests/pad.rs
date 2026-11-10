//! Tests of the native-gesture multi-touch pad.

use super::support::*;
use crate::input::*;

/// A `touchpad_raw` message parses with id, x and y per slot; pressure is optional and defaults to 0; an empty slot list is valid.
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

/// Fingers get slots and tracking ids when they land and are lifted with tracking id -1.
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
    assert!(
        down.contains(&(EV_ABS, ABS_MT_POSITION_X, 400)),
        "10 mm = 400 units"
    );
    assert!(down.contains(&(EV_ABS, ABS_MT_POSITION_Y, 800)));
    assert!(
        down.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 1)) && down.contains(&(EV_KEY, BTN_TOUCH, 1))
    );

    let moved = &f[1];
    assert!(
        !moved.iter().any(|e| e.1 == ABS_MT_TRACKING_ID),
        "tracking ids are only sent when a finger lands"
    );
    assert!(moved.contains(&(EV_ABS, ABS_MT_POSITION_X, 440)));

    let one_left = &f[2];
    assert!(one_left.contains(&(EV_ABS, ABS_MT_SLOT, 0)));
    assert!(
        one_left.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)),
        "slot 0 lifted"
    );
    assert!(one_left.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 0)));
    assert!(one_left.contains(&(EV_KEY, BTN_TOOL_FINGER, 1)));

    let none = &f[3];
    assert!(
        none.contains(&(EV_ABS, ABS_MT_SLOT, 1))
            && none.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1))
    );
    assert!(none.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)) && none.contains(&(EV_KEY, BTN_TOUCH, 0)));
    assert_eq!(st.count(), 0);
}

/// A freed slot is reused with a new tracking id, and positions are clamped to the pad surface.
#[test]
fn pad_reuses_a_freed_slot_with_a_fresh_tracking_id_and_clamps_to_the_surface() {
    let (mut dev, path) = device_over_file("padreuse");
    let mut st = PadState::new();
    dev.inject_pad(&mut st, &[finger(1, 5.0, 5.0)]).unwrap();
    dev.inject_pad(&mut st, &[]).unwrap();
    dev.inject_pad(&mut st, &[finger(2, 9999.0, -50.0)])
        .unwrap();
    let f = frames(&path);
    let _ = std::fs::remove_file(&path);
    let again = &f[2];
    assert!(
        again.contains(&(EV_ABS, ABS_MT_SLOT, 0)),
        "the lowest free slot"
    );
    assert!(
        again.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 1)),
        "never reuses a tracking id"
    );
    assert!(
        again.contains(&(EV_ABS, ABS_MT_POSITION_X, 640 * 40 - 1)),
        "clamped to the pad"
    );
    assert!(again.contains(&(EV_ABS, ABS_MT_POSITION_Y, 0)));
}

/// Three, four and five fingers select the triple-tap, quad-tap and quint-tap tool buttons.
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
    assert!(
        f[1].contains(&(EV_KEY, BTN_TOOL_TRIPLETAP, 0))
            && f[1].contains(&(EV_KEY, BTN_TOOL_QUADTAP, 1))
    );
    assert!(f[2].contains(&(EV_KEY, BTN_TOOL_QUINTTAP, 1)));
}

/// Leaving touchpad mode removes the pad device and first lifts its fingers.
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
    assert!(
        f[0].contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)),
        "the finger was lifted first"
    );
    assert_eq!(d.pad_state.count(), 0);
}

/// Native gestures are offered only on a Wayland GNOME or KDE session.
#[test]
fn native_gestures_need_wayland_gnome_or_kde() {
    use crate::config::options::native_gestures_supported as ok;
    assert!(ok("wayland", "ubuntu:GNOME"));
    assert!(ok("wayland", "GNOME"));
    assert!(ok("Wayland", "KDE"));
    assert!(
        !ok("x11", "GNOME"),
        "X11 never turns libinput gestures into actions"
    );
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
    let travel = |d: &mut InjectDevices,
                  n: usize,
                  cx: f64,
                  cy: f64,
                  spread: f64,
                  dx: f64,
                  dy: f64,
                  steps: u32| {
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
            vec![
                finger(1, 200.0 - g / 2.0, 120.0),
                finger(2, 200.0 + g / 2.0, 120.0),
            ]
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
