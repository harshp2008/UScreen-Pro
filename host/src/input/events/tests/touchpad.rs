//! Tests of touchpad-mode events, key combos and gesture messages.

use super::support::*;
use crate::input::*;

/// A pinch holds Ctrl for every step and lets go at the end.
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

/// Touchpad motion and clicks become mouse events; zero motion writes no frame.
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

/// Scrolling sends hi-res units and whole notches from the accumulated remainder.
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

/// Touchpad messages (`rel`, `button`, `scroll`) parse.
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

/// Key combinations press modifiers first and release in reverse order.
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

/// In stylus mode there is no relative mouse device.
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

/// Entering touchpad mode lifts the stylus policy and keeps the devices that exist.
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

/// Gesture messages parse.
#[test]
fn gesture_messages_parse() {
    let g: InputEvent =
        serde_json::from_str(r#"{"type":"gesture","kind":"pinch","amount":-12.5}"#).unwrap();
    assert!(
        matches!(g, InputEvent::Gesture { ref kind, amount } if kind == "pinch" && amount == -12.5)
    );
}
