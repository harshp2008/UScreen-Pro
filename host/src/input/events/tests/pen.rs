//! Tests of pen sample conversion and styles.

use super::support::*;
use crate::input::*;

/// A pen lift sends its last position before it leaves proximity.
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

/// A touching pen never reports pressure 0, because the kernel would see that as lifted.
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

/// In mouse style the left button is held from pen down to pen up.
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
    assert_eq!(
        left.len(),
        2,
        "one press and one release, nothing in between: {left:?}"
    );
    assert_eq!(left[0].1, 1);
    assert_eq!(left[1].1, 0);
    let moves_while_held = f[left[0].0..left[1].0]
        .iter()
        .filter(|fr| fr.iter().any(|e| e.0 == EV_ABS && e.1 == ABS_X))
        .count();
    assert!(
        moves_while_held >= 10,
        "{moves_while_held} position frames while the button was held"
    );
    assert_eq!(inj.pen_mouse_buttons, 0);
}

/// The pen's side buttons follow the configured mapping.
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

/// Lifting the eraser end leaves proximity with the eraser tool.
#[test]
fn eraser_lift_leaves_proximity_with_the_eraser_tool() {
    let (mut dev, path) = device_over_file("eraserlift");
    dev.inject_pen(10, 20, 500, 0, 0, 0, true).unwrap();
    dev.inject_pen(10, 20, 0, 0, 0, 1, true).unwrap();
    let f = frames(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(f.last().unwrap(), &vec![(EV_KEY, BTN_TOOL_RUBBER, 0)]);
}
