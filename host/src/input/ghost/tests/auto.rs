//! Tests of the Android auto style: per-stroke routing, repeated/lost DOWNs, click slop, and the
//! parking of a resting pen.

use super::super::auto::{AUTO_EXIT_GRACE, AUTO_STALE_STROKE};
use super::super::*;
use super::support::*;
use std::time::Duration;

#[test]
fn auto_mouse_route_stroke_is_pointer_only_position_first_then_one_press_held_to_the_lift() {
    // Android: hover, range exit just before the touch-down, down, moves, up. Unknown focus is
    // the same route (`to_pen == false`).
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 9000, false);
    auto(&mut d, 4, 0, false);
    auto(&mut d, 0, 10000, false);
    for i in 1..=4 {
        auto(&mut d, 2, 10000 + i * 1000, false);
    }
    auto(&mut d, 1, 14000, false);
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert!(
        pen.iter().flatten().next().is_none(),
        "the tablet device gets nothing at all on the mouse route"
    );
    assert_eq!(
        left_events(&ptr),
        vec![1, 0],
        "one press, no extra press or release during the drag, one release"
    );
    let press = ptr
        .iter()
        .position(|f| f.contains(&(EV_KEY, BTN_LEFT, 1)))
        .unwrap();
    let at_down = ptr
        .iter()
        .position(|f| f.contains(&(EV_ABS, ABS_X, 10000)))
        .unwrap();
    assert!(
        at_down < press,
        "ABS_X/Y reach the pointer before BTN_LEFT goes down"
    );
    let release = ptr
        .iter()
        .position(|f| f.contains(&(EV_KEY, BTN_LEFT, 0)))
        .unwrap();
    for i in 1..=4 {
        let m = ptr
            .iter()
            .position(|f| f.contains(&(EV_ABS, ABS_X, 10000 + i * 1000)))
            .unwrap();
        assert!(
            press < m && m < release,
            "move {} is between the press and the release",
            i
        );
    }
    assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
}

#[test]
fn auto_art_route_stroke_is_the_tablet_with_real_pressure_and_no_click() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 100, true);
    auto(&mut d, 4, 0, true);
    auto(&mut d, 0, 100, true);
    auto(&mut d, 2, 120, true);
    auto(&mut d, 1, 120, true);
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert!(has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)));
    assert!(has(&pen, (EV_KEY, BTN_TOUCH, 1)) && has(&pen, (EV_KEY, BTN_TOUCH, 0)));
    let pressures: Vec<i32> = pen
        .iter()
        .flatten()
        .filter(|e| e.1 == ABS_PRESSURE && e.2 > 0)
        .map(|e| e.2)
        .collect();
    assert!(
        !pressures.is_empty() && pressures.iter().all(|p| *p == 2000),
        "real pressure, never squeezed: {:?}",
        pressures
    );
    assert!(has(&pen, (EV_ABS, ABS_TILT_X, 3)));
    assert!(
        !has(&ptr, (EV_KEY, BTN_LEFT, 1)),
        "no left click in an art app"
    );
    assert!(has(&ptr, (EV_ABS, ABS_X, 120)), "the cursor still follows");
    assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
}

#[test]
fn auto_art_route_eraser_end_draws_with_the_rubber_tool() {
    let (mut d, pp, qp) = rig();
    let mut s = sample(0, 100);
    s.eraser = true;
    d.auto_pen(&s, true, "right", "middle");
    let pen = frames(&pp);
    done(&[&pp, &qp]);
    assert!(has(&pen, (EV_KEY, BTN_TOOL_RUBBER, 1)) && has(&pen, (EV_KEY, BTN_TOUCH, 1)));
}

#[test]
fn auto_mouse_route_eraser_end_never_clicks() {
    let (mut d, pp, qp) = rig();
    let mut s = sample(0, 100);
    s.eraser = true;
    d.auto_pen(&s, false, "right", "middle");
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert!(left_events(&ptr).is_empty());
}

#[test]
fn auto_a_focus_change_in_mid_stroke_neither_cuts_nor_moves_the_stroke() {
    // Chrome (mouse) stroke, the focus flips to Krita and back while the button is down.
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 10, false);
    auto(&mut d, 2, 20, true);
    auto(&mut d, 2, 30, false);
    auto(&mut d, 2, 40, true);
    d.auto_check_route(true);
    d.auto_flush_exit();
    assert_eq!(
        d.dev_state(Which::Pointer),
        DevState::Down(0b1),
        "still held"
    );
    auto(&mut d, 1, 40, true);
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert_eq!(left_events(&ptr), vec![1, 0]);
    assert!(
        pen.iter().flatten().next().is_none(),
        "the stroke stayed on the pointer"
    );

    // And the other way: a Krita stroke stays a tablet stroke when the focus leaves.
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 10, true);
    auto(&mut d, 2, 20, false);
    d.auto_check_route(false);
    assert_eq!(
        d.dev_state(Which::Pen),
        DevState::Down(0b1000),
        "the contact is still down"
    );
    auto(&mut d, 2, 30, false);
    auto(&mut d, 1, 30, false);
    let (pen, ptr) = (frames(&pp), frames(&qp));
    done(&[&pp, &qp]);
    assert!(!has(&ptr, (EV_KEY, BTN_LEFT, 1)));
    let lift = pen
        .iter()
        .position(|f| f.contains(&(EV_KEY, BTN_TOUCH, 0)))
        .unwrap();
    let last_move = pen
        .iter()
        .rposition(|f| f.contains(&(EV_ABS, ABS_X, 30)))
        .unwrap();
    assert!(
        last_move <= lift,
        "every move before the lift reached the tablet"
    );
    assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
}

#[test]
fn auto_a_repeated_down_in_mid_drag_is_a_move_not_a_release_and_press() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 1000, false);
    auto(&mut d, 0, 1000, false);
    auto(&mut d, 2, 2000, false);
    auto(&mut d, 0, 3000, false); // a DOWN again, long after the first? no: while the drag runs
    auto(&mut d, 2, 4000, false);
    auto(&mut d, 1, 4000, false);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(
        left_events(&ptr),
        vec![1, 0],
        "the hold is never interrupted"
    );
    assert!(
        has(&ptr, (EV_ABS, ABS_X, 3000)),
        "the repeated DOWN's position still moves the cursor"
    );
}

#[test]
fn auto_a_down_after_a_lost_up_closes_the_old_stroke_first() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 10, false);
    std::thread::sleep(AUTO_STALE_STROKE + Duration::from_millis(30));
    auto(&mut d, 0, 90, false); // the UP in between never arrived
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(left_events(&ptr), vec![1, 0, 1]);
}

#[test]
fn auto_a_move_whose_down_was_lost_starts_the_stroke() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 10, false);
    auto(&mut d, 2, 20, false); // no DOWN ever arrived
    auto(&mut d, 2, 30, false);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
    auto(&mut d, 1, 30, false);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(left_events(&ptr), vec![1, 0]);
}

#[test]
fn auto_a_straggler_move_right_after_the_lift_does_not_press_again() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 10, false);
    auto(&mut d, 1, 10, false);
    auto(&mut d, 2, 15, false); // arrived late
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(left_events(&ptr), vec![1, 0]);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
}

#[test]
fn auto_a_range_exit_inside_a_stroke_does_not_let_go() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 10, false);
    auto(&mut d, 4, 0, false);
    std::thread::sleep(AUTO_EXIT_GRACE + Duration::from_millis(20));
    d.auto_flush_exit();
    assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
    auto(&mut d, 2, 20, false);
    auto(&mut d, 1, 20, false);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(left_events(&ptr), vec![1, 0]);
}

#[test]
fn auto_range_exit_before_the_tip_keeps_a_held_barrel_button() {
    // Android: barrel held, hover exit, DOWN within a few ms: a right-drag.
    let (mut d, pp, qp) = rig();
    auto(&mut d, 5, 0, false);
    auto(&mut d, 4, 0, false);
    d.auto_flush_exit();
    auto(&mut d, 0, 10, false);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b11));
    done(&[&pp, &qp]);
}

#[test]
fn auto_range_exit_that_lasts_leaves_both_devices_idle_including_the_right_button() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 5, 0, false); // barrel held
    auto(&mut d, 3, 10, true);
    auto(&mut d, 4, 0, true);
    std::thread::sleep(AUTO_EXIT_GRACE + Duration::from_millis(20));
    d.auto_flush_exit();
    done(&[&pp, &qp]);
    assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
    assert!(d.auto_exit_at.is_none());
}

#[test]
fn auto_pen_up_lets_go_on_both_devices() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 10, false);
    auto(&mut d, 1, 10, false);
    auto(&mut d, 0, 20, true);
    auto(&mut d, 1, 20, true);
    done(&[&pp, &qp]);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
    assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
    assert_eq!(d.pen_mouse_buttons, 0);
    assert!(!d.pen_proximity);
}

#[test]
fn auto_hover_follows_the_route_the_tool_in_range_only_on_the_art_route() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 100, true);
    assert!(d.pen_proximity);
    auto(&mut d, 3, 110, false);
    assert!(!d.pen_proximity, "left the art app: the tool leaves range");
    auto(&mut d, 3, 120, true);
    assert!(d.pen_proximity);
    d.auto_check_route(false); // the focus moved with the pen held still
    assert!(!d.pen_proximity);
    done(&[&pp, &qp]);
}

#[test]
fn auto_barrel_button_follows_the_route() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 5, 0, false);
    assert_eq!(d.pen_mouse_buttons & 2, 2, "mouse route: right button");
    auto(&mut d, 6, 0, false);
    assert_eq!(d.pen_mouse_buttons, 0);
    auto(&mut d, 5, 0, true);
    let pen = frames(&pp);
    done(&[&pp, &qp]);
    assert!(
        has(&pen, (EV_KEY, BTN_STYLUS, 1)),
        "art route: the tablet's barrel button"
    );
}

#[test]
fn slop_a_wobble_inside_the_zone_never_reaches_the_pointer_and_the_lift_releases_at_the_press_point(
) {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 5000, false);
    auto(&mut d, 0, 5000, false);
    let before = xs(&frames(&qp)).len();
    for dx in [60, -80, 120, -100, 150] {
        auto(&mut d, 2, 5000 + dx, false); // under 1 mm
    }
    auto(&mut d, 1, 5100, false);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(
        xs(&ptr).len(),
        before,
        "no pointer motion while inside the slop, not even at the lift"
    );
    assert_eq!(left_events(&ptr), vec![1, 0], "a clean click");
}

#[test]
fn slop_leaving_the_zone_releases_the_pointer_for_the_rest_of_the_stroke() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 5000, false);
    auto(&mut d, 2, 5100, false); // inside
    auto(&mut d, 2, 8000, false); // ~12 mm: out
    auto(&mut d, 2, 5050, false); // back near the start: it is a drag now, so it follows
    auto(&mut d, 1, 5050, false);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    let x = xs(&ptr);
    assert!(x.contains(&8000) && x.last() == Some(&5050), "{:?}", x);
    assert!(!x.contains(&5100), "the wobble inside the zone was held");
}

#[test]
fn slop_does_not_apply_on_the_art_route() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 5000, true);
    auto(&mut d, 2, 5060, true);
    let ptr = frames(&qp);
    let pen = frames(&pp);
    done(&[&pp, &qp]);
    assert!(xs(&ptr).contains(&5060), "the cursor follows every sample");
    assert!(has(&pen, (EV_ABS, ABS_X, 5060)), "and so does the tablet");
}

#[test]
fn slop_is_cleared_when_the_stroke_ends() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 0, 5000, false);
    auto(&mut d, 1, 5000, false);
    assert!(d.slop_anchor.is_none());
    auto(&mut d, 3, 5030, false);
    assert!(xs(&frames(&qp)).contains(&5030), "hovering follows at once");
    done(&[&pp, &qp]);
}

#[test]
fn auto_a_resting_pen_is_parked_and_stops_touching_the_cursor() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 5000, false);
    auto(&mut d, 3, 5010, false); // noise
    std::thread::sleep(d.still_cfg.park_after + Duration::from_millis(40));
    auto(&mut d, 3, 5005, false); // parks here
    let writes_when_parked = frames(&qp).len();
    for i in 0..20 {
        auto(&mut d, 3, 5000 + (i % 7), false); // a parked pen's noise
    }
    let after = frames(&qp).len();
    done(&[&pp, &qp]);
    assert!(writes_when_parked >= 1, "it was followed before parking");
    assert_eq!(
        after, writes_when_parked,
        "nothing more is written to the pointer while parked"
    );
}

#[test]
fn auto_a_parked_pen_on_the_art_route_leaves_range_and_wakes_on_clear_movement() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 5000, true);
    assert!(d.pen_proximity);
    std::thread::sleep(d.still_cfg.park_after + Duration::from_millis(40));
    auto(&mut d, 3, 5002, true);
    assert!(
        !d.pen_proximity,
        "parked: the tablet tool is out of range and cannot own the cursor"
    );
    auto(&mut d, 3, 5003, true);
    assert!(!d.pen_proximity);
    auto(&mut d, 3, 5000 + 4000, true); // ~ 16 mm away
    assert!(d.pen_proximity, "woken by clear movement: the tool is back");
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert!(has(&ptr, (EV_ABS, ABS_X, 9000)));
}

#[test]
fn auto_touching_down_works_from_a_parked_pen() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 3, 5000, false);
    std::thread::sleep(d.still_cfg.park_after + Duration::from_millis(40));
    auto(&mut d, 3, 5001, false);
    auto(&mut d, 4, 0, false);
    auto(&mut d, 0, 5001, false);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
    auto(&mut d, 1, 5001, false);
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert_eq!(left_events(&ptr), vec![1, 0]);
}
