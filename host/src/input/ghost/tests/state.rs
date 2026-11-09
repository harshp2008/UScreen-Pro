//! Tests of the tracked device state (`DevState`) and of the forced releases.

use super::super::*;
use super::support::*;

#[test]
fn device_state_follows_what_the_device_holds() {
    let (mut d, pp, qp) = rig();
    assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
    auto(&mut d, 3, 10, true);
    assert_eq!(d.dev_state(Which::Pen), DevState::Hovering);
    auto(&mut d, 0, 10, true);
    assert_eq!(d.dev_state(Which::Pen), DevState::Down(0b1000), "tip contact");
    auto(&mut d, 1, 10, true);
    assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
    auto(&mut d, 0, 20, false);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
    done(&[&pp, &qp]);
}

#[test]
fn force_release_all_lets_go_of_everything_a_device_holds() {
    let (mut d, pp, qp) = rig();
    auto(&mut d, 5, 0, false); // barrel -> right button on the pointer
    auto(&mut d, 0, 10, false);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b11));
    d.force_release_all(Which::Pointer);
    assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
    assert!(d.auto_down_at.is_none(), "the stroke is closed with it");
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    assert!(has(&ptr, (EV_KEY, BTN_LEFT, 0)) && has(&ptr, (EV_KEY, BTN_RIGHT, 0)));
    assert_eq!(d.pen_mouse_buttons, 0);
    assert_eq!(d.tip, Tip::None);
}

#[test]
fn a_second_left_down_on_the_pointer_device_is_written_as_it_comes() {
    // The device layer no longer rewrites a press (an inserted release would break a hold).
    let (mut d, pp, qp) = rig();
    {
        let dev = d.pointer.as_mut().unwrap();
        dev.emit(EV_KEY, BTN_LEFT, 1).unwrap();
        dev.syn().unwrap();
        dev.emit(EV_KEY, BTN_LEFT, 1).unwrap();
        dev.syn().unwrap();
    }
    let ptr = frames(&qp);
    done(&[&pp, &qp]);
    let raw: Vec<i32> = ptr.iter().flatten().filter(|e| e.1 == BTN_LEFT).map(|e| e.2).collect();
    assert_eq!(raw, vec![1, 1]);
}

#[test]
fn a_lift_that_does_not_name_the_eraser_still_releases_the_rubber_tool() {
    // Plain tablet path: DOWN with the eraser end, then an UP sample (a cancel) without it.
    let (mut d, pp, qp) = rig();
    {
        let pen = d.pen.as_mut().unwrap();
        pen.inject_pen(10, 10, 500, 0, 0, 0, true).unwrap();
        assert_ne!(pen.held & key_bit(BTN_TOOL_RUBBER).unwrap(), 0);
        pen.inject_pen(10, 10, 0, 0, 0, 1, false).unwrap();
        assert_eq!(pen.state(), DevState::Idle, "no tool key left down");
    }
    done(&[&pp, &qp]);
}
