//! Tests of the physical-tablet injection (`ghost_pen`): hover, clicks, side buttons, panic release,
//! idle cursor hide.

use super::super::*;
use super::support::*;
use std::time::{Duration, Instant};

/// On the pen route hover is written to both devices, so the system cursor stays visible while the tablet tool is in range.
#[test]
fn hover_moves_both_devices_on_the_pen_route_so_the_cursor_stays_visible() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(3, 100), true, "right", "middle");
    d.ghost_pen(&sample(3, 200), true, "right", "middle");
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert_eq!(pen.len(), 2);
    assert_eq!(ptr.len(), 2, "one pointer motion frame per hover sample");
    assert!(has(&pen, (EV_ABS, ABS_X, 200)) && has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)));
    assert!(has(&ptr, (EV_ABS, ABS_X, 200)));
    assert!(!has(&ptr, (EV_KEY, BTN_LEFT, 1)), "hover never clicks");
}

/// On the mouse route the tablet tool is never put in range, across two full taps of the Android sequence.
#[test]
fn the_mouse_route_never_puts_the_tablet_tool_in_range() {
    // The Android sequence of a tap: hover, range exit (just before the tip lands), down, a
    // move, up, hover again - then a second tap.
    let (mut d, pp, qp) = rig();
    for _ in 0..2 {
        d.ghost_pen(&sample(3, 100), false, "right", "middle");
        d.ghost_pen(&sample(4, 0), false, "right", "middle");
        d.ghost_pen(&sample(0, 100), false, "right", "middle");
        d.ghost_pen(&sample(2, 110), false, "right", "middle");
        d.ghost_pen(&sample(1, 110), false, "right", "middle");
    }
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert!(
        !has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)),
        "no proximity-in, ever: clients would treat it as a pen session"
    );
    assert!(!has(&pen, (EV_KEY, BTN_TOUCH, 1)));
    assert!(
        !pen.iter().flatten().any(|e| e.0 == EV_ABS && e.1 == ABS_X),
        "no tablet motion either"
    );
    let left: Vec<i32> = ptr
        .iter()
        .flat_map(|f| {
            f.iter()
                .filter(|e| e.1 == BTN_LEFT && e.2 == 1)
                .map(|e| e.2)
        })
        .collect();
    assert_eq!(left.len(), 2, "both taps click");
    assert!(has(&ptr, (EV_ABS, ABS_X, 110)));
    assert_eq!(d.pen_mouse_buttons, 0);
}

/// Flipping the route during hover puts the tablet tool in range or takes it out of range accordingly.
#[test]
fn a_route_flip_while_hovering_moves_the_tool_in_and_out_of_range() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(3, 100), true, "right", "middle");
    assert!(d.pen_proximity);
    d.ghost_pen(&sample(3, 110), false, "right", "middle");
    assert!(
        !d.pen_proximity,
        "left the pen route: the tool leaves range"
    );
    let pen = frames(&pp);
    done(&[&pp, &qp]);
    assert!(has(&pen, (EV_KEY, BTN_TOOL_PEN, 0)));
}

/// A repeated hover position is written one unit to the side (500, 501, 500) and never as the identical value twice, so the pointer device still sees motion.
#[test]
fn a_still_pen_keeps_the_pointer_alive_with_a_one_unit_nudge() {
    let (mut d, pp, qp) = rig();
    for _ in 0..3 {
        d.ghost_pen(&sample(3, 500), true, "right", "middle");
    }
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    let xs: Vec<i32> = ptr
        .iter()
        .flat_map(|f| f.iter().filter(|e| e.1 == ABS_X).map(|e| e.2))
        .collect();
    assert_eq!(
        xs,
        vec![500, 501, 500],
        "a repeat is written beside the position, never twice the same"
    );
}

/// The panic release lifts every button and tool on both devices.
#[test]
fn panic_release_lets_go_of_everything_on_both_devices() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(0, 100), false, "right", "middle"); // left held
    d.ghost_pen(&sample(5, 0), false, "right", "middle"); // barrel -> right held
    assert_eq!(d.pen_mouse_buttons, 1 | 2);
    d.ghost_panic_release(false);
    assert_eq!(
        d.pen_mouse_buttons, 2,
        "the barrel button keeps its own lifecycle"
    );
    d.ghost_panic_release(true);
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert_eq!(d.pen_mouse_buttons, 0);
    assert_eq!(d.tip, Tip::None);
    for e in [
        (EV_KEY, BTN_LEFT, 0),
        (EV_KEY, BTN_RIGHT, 0),
        (EV_KEY, BTN_MIDDLE, 0),
    ] {
        assert!(has(&ptr, e), "pointer: {e:?}");
    }
    for e in [
        (EV_KEY, BTN_TOUCH, 0),
        (EV_KEY, BTN_STYLUS, 0),
        (EV_KEY, BTN_TOOL_PEN, 0),
        (EV_KEY, BTN_TOOL_RUBBER, 0),
    ] {
        assert!(has(&pen, e), "pen: {e:?}");
    }
}

/// A lift with no earlier DOWN still releases the left button, so a lost DOWN cannot leave the button stuck.
#[test]
fn a_lift_with_no_matching_down_still_releases_the_left_button() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(1, 100), false, "right", "middle");
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert!(has(&ptr, (EV_KEY, BTN_LEFT, 0)));
}

/// For an app that is not a drawing app, the tip is a left click on the pointer device only.
#[test]
fn non_art_app_tip_is_a_left_click_on_the_pointer_only() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(3, 100), false, "right", "middle");
    d.ghost_pen(&sample(0, 100), false, "right", "middle");
    for i in 1..=5 {
        d.ghost_pen(&sample(2, 100 + i * 10), false, "right", "middle");
    }
    d.ghost_pen(&sample(1, 150), false, "right", "middle");
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    let left: Vec<i32> = ptr
        .iter()
        .flat_map(|f| f.iter().filter(|e| e.1 == BTN_LEFT).map(|e| e.2))
        .collect();
    assert_eq!(left.iter().filter(|v| **v == 1).count(), 1, "one press");
    assert_eq!(left.first(), Some(&1));
    assert_eq!(
        left.last(),
        Some(&0),
        "and it ends released (a redundant release is dropped by the kernel)"
    );
    assert!(
        !has(&pen, (EV_KEY, BTN_TOUCH, 1)),
        "the tablet never touches"
    );
    assert!(
        pen.iter()
            .all(|f| !f.contains(&(EV_ABS, ABS_PRESSURE, 2000))),
        "no pressure reaches the tablet in a non-art app"
    );
    assert!(
        !has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)),
        "and it stays out of range"
    );
    assert!(
        has(&ptr, (EV_ABS, ABS_X, 150)),
        "the pointer follows to the lift point"
    );
    assert_eq!(d.pen_mouse_buttons, 0);
}

/// For a drawing app the tip goes to the tablet device with pressure and never produces a click.
#[test]
fn art_app_tip_goes_to_the_pen_with_pressure_and_never_clicks() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(3, 100), true, "right", "middle");
    d.ghost_pen(&sample(0, 100), true, "right", "middle");
    d.ghost_pen(&sample(2, 120), true, "right", "middle");
    d.ghost_pen(&sample(1, 130), true, "right", "middle");
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert!(has(&pen, (EV_KEY, BTN_TOUCH, 1)) && has(&pen, (EV_KEY, BTN_TOUCH, 0)));
    assert!(has(&pen, (EV_ABS, ABS_PRESSURE, 2000)));
    assert!(
        !ptr.iter().flatten().any(|e| e.0 == EV_KEY && e.2 == 1),
        "the pointer presses no button"
    );
    assert!(
        has(&ptr, (EV_ABS, ABS_X, 130)),
        "but follows the pen to the lift point"
    );
}

/// A stroke finishes on the device it started on, even if the route flips in the middle.
#[test]
fn a_stroke_stays_on_the_device_it_started_on_when_the_route_flips() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(0, 100), false, "right", "middle"); // started as a mouse click
    d.ghost_pen(&sample(2, 120), true, "right", "middle"); // the route flips mid-stroke
    d.ghost_pen(&sample(1, 130), true, "right", "middle");
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    let left: Vec<i32> = ptr
        .iter()
        .flat_map(|f| f.iter().filter(|e| e.1 == BTN_LEFT).map(|e| e.2))
        .collect();
    assert_eq!(left.iter().filter(|v| **v == 1).count(), 1);
    assert_eq!(left.last(), Some(&0), "released on the pointer");
    assert!(
        !has(&pen, (EV_KEY, BTN_TOUCH, 1)),
        "never touched the tablet"
    );
}

/// The pen's side button goes to the tablet in drawing apps and to the pointer elsewhere.
#[test]
fn side_button_goes_to_the_pen_in_art_apps_and_to_the_pointer_elsewhere() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(5, 0), true, "right", "middle");
    d.ghost_pen(&sample(6, 0), true, "right", "middle");
    d.ghost_pen(&sample(5, 0), false, "right", "middle");
    d.ghost_pen(&sample(6, 0), false, "right", "middle");
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert!(has(&pen, (EV_KEY, BTN_STYLUS, 1)) && has(&pen, (EV_KEY, BTN_STYLUS, 0)));
    assert!(has(&ptr, (EV_KEY, BTN_RIGHT, 1)) && has(&ptr, (EV_KEY, BTN_RIGHT, 0)));
    assert_eq!(d.pen_mouse_buttons, 0);
}

/// Leaving range stops pointer updates and starts the idle clock.
#[test]
fn leaving_range_stops_pointer_updates_and_starts_the_idle_clock() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(3, 100), false, "right", "middle");
    d.ghost_pen(&sample(4, 100), false, "right", "middle");
    let ptr = frames(&qp);
    let pen = frames(&pp);
    done(&[&pp, &qp]);
    assert_eq!(
        ptr.iter().flatten().filter(|e| e.1 == ABS_X).count(),
        1,
        "range exit moves nothing on the pointer"
    );
    assert!(has(&pen, (EV_KEY, BTN_TOOL_PEN, 0)));
    assert!(d.ghost_out_since.is_some());
    assert!(!d.pen_proximity);
}

/// Idle hide is off at 0 and otherwise fires once after the timeout with no pointer motion.
#[test]
fn idle_hide_is_off_at_zero_and_fires_once_after_the_timeout_with_no_pointer_motion() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(3, 321), false, "right", "middle");
    d.ghost_pen(&sample(4, 321), false, "right", "middle");
    assert!(!d.ghost_idle_tick(0), "0 means never");
    assert!(!d.ghost_idle_tick(2), "too early");
    d.ghost_out_since = Some(Instant::now() - Duration::from_secs(3));
    let ptr_before = frames(&qp).len();
    assert!(d.ghost_idle_tick(2));
    assert!(!d.ghost_idle_tick(2), "only once");
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert_eq!(ptr.len(), ptr_before, "the hide never touches the pointer");
    let n = pen.len();
    assert_eq!(
        pen[n - 2],
        vec![
            (EV_KEY, BTN_TOOL_PEN, 1),
            (EV_ABS, ABS_X, 321),
            (EV_ABS, ABS_Y, 500)
        ]
    );
    assert_eq!(pen[n - 1], vec![(EV_KEY, BTN_TOOL_PEN, 0)]);
    // Coming back shows the cursor again and re-arms the clock.
    let (mut d2, p2, q2) = rig();
    d2.ghost_out_since = Some(Instant::now() - Duration::from_secs(9));
    d2.ghost_hidden = true;
    d2.ghost_pen(&sample(3, 5), false, "right", "middle");
    assert!(d2.ghost_out_since.is_none() && !d2.ghost_hidden);
    assert!(frames(&q2).iter().flatten().any(|e| e.1 == ABS_X));
    done(&[&p2, &q2]);
}

/// A range exit releases a held tip.
#[test]
fn range_exit_releases_a_held_tip() {
    let (mut d, pp, qp) = rig();
    d.ghost_pen(&sample(0, 10), false, "right", "middle");
    d.ghost_pen(&sample(4, 10), false, "right", "middle");
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert!(has(&ptr, (EV_KEY, BTN_LEFT, 0)));
    assert_eq!(d.pen_mouse_buttons, 0);
    assert_eq!(d.tip, Tip::None);
}

/// With a known screen size (1920 x 1080 in the test) the pointer only lands on whole pixels; fractional positions leave stale cursor images behind with software cursors.
#[test]
fn the_pointer_only_ever_lands_on_whole_pixels_when_the_screen_size_is_known() {
    // Fractional positions leave stale "ghost" cursor images behind with software cursors, so the
    // absolute pointer is snapped to the panel's pixel grid (here 1920 x 1080).
    let (mut d, pp, qp) = rig();
    d.snap_grid = Some((1920.0, 1080.0));
    for i in 0..400 {
        d.ghost_place_pointer(10_000 + i * 53 + (i % 7), 20_000 + i * 31 + (i % 5));
    }
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    let px = |v: i32, n: f64| {
        let c = v as f64 / COORD_MAX as f64 * (n - 1.0);
        (c - c.round()).abs()
    };
    let abs_vals = |code: u16| -> Vec<i32> {
        ptr.iter()
            .flatten()
            .filter(|e| e.0 == EV_ABS && e.1 == code)
            .map(|e| e.2)
            .collect()
    };
    let (xs_, ys_) = (abs_vals(ABS_X), abs_vals(ABS_Y));
    assert!(!xs_.is_empty());
    assert!(
        xs_.iter().all(|&v| px(v, 1920.0) < 0.02),
        "x lands between pixels"
    );
    assert!(
        ys_.iter().all(|&v| px(v, 1080.0) < 0.02),
        "y lands between pixels"
    );
}

/// Without a known screen size the positions are written unchanged.
#[test]
fn without_a_known_screen_size_positions_are_left_alone() {
    let (mut d, pp, qp) = rig();
    d.snap_grid = None;
    d.ghost_place_pointer(12_345, 23_456);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert!(xs(&ptr).contains(&12_345));
}
