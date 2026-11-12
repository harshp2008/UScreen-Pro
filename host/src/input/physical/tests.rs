//! Unit tests of `physical.rs`. Moved out of the file so the code and its tests read separately; they see its private items through `use super::*`.

use super::*;

/// A pen state with a 1000 x 500 x 8000 range and 0.5 degrees per tilt unit.
fn st() -> PhysState {
    PhysState::new(AbsRange {
        x_max: 1000,
        y_max: 500,
        p_max: 8000,
        tilt_deg_per_unit: 0.5,
    })
}

/// Sends `SYN_REPORT` and returns the samples it completes.
fn syn(s: &mut PhysState) -> Vec<PenSample> {
    s.feed(EV_SYN, SYN_REPORT, 0)
}

/// The action codes of `v`, in order.
fn actions(v: &[PenSample]) -> Vec<u8> {
    v.iter().map(|s| s.action).collect()
}

/// A full stroke yields hover, down, moves, up, hover, then out-of-range, in that order, one sample per report.
#[test]
fn a_stroke_is_hover_down_moves_up_hover_then_out_of_range() {
    let mut s = st();
    s.feed(EV_KEY, BTN_TOOL_PEN, 1);
    s.feed(EV_ABS, ABS_X, 500);
    s.feed(EV_ABS, ABS_Y, 250);
    assert_eq!(actions(&syn(&mut s)), vec![3], "entering range hovers");
    s.feed(EV_ABS, ABS_X, 510);
    assert_eq!(actions(&syn(&mut s)), vec![3], "moving in range hovers");
    assert!(syn(&mut s).is_empty(), "an empty report says nothing");
    s.feed(EV_KEY, BTN_TOUCH, 1);
    s.feed(EV_ABS, ABS_PRESSURE, 4000);
    let down = syn(&mut s);
    assert_eq!(actions(&down), vec![0]);
    assert_eq!(
        down[0].pressure,
        PEN_PRESSURE_MAX / 2,
        "4000 of 8000 is half of the virtual axis"
    );
    s.feed(EV_ABS, ABS_X, 600);
    assert_eq!(actions(&syn(&mut s)), vec![2]);
    s.feed(EV_KEY, BTN_TOUCH, 0);
    s.feed(EV_ABS, ABS_PRESSURE, 0);
    assert_eq!(actions(&syn(&mut s)), vec![1]);
    s.feed(EV_ABS, ABS_X, 620);
    assert_eq!(
        actions(&syn(&mut s)),
        vec![3],
        "hovering again after the lift"
    );
    s.feed(EV_KEY, BTN_TOOL_PEN, 0);
    assert_eq!(actions(&syn(&mut s)), vec![4]);
    assert!(syn(&mut s).is_empty(), "nothing more once out of range");
}

/// Position, pressure and tilt come out scaled to the virtual pen's range.
#[test]
fn positions_pressure_and_tilt_are_scaled_to_the_virtual_pen() {
    let mut s = st();
    s.feed(EV_KEY, BTN_TOOL_PEN, 1);
    s.feed(EV_ABS, ABS_X, 1000);
    s.feed(EV_ABS, ABS_Y, 250);
    s.feed(EV_ABS, ABS_TILT_X, -60);
    s.feed(EV_ABS, ABS_TILT_Y, 400); // 200 deg raw: clamps to 90
    let out = syn(&mut s);
    assert_eq!((out[0].x, out[0].y), (COORD_MAX, COORD_MAX / 2));
    assert_eq!((out[0].tilt_x, out[0].tilt_y), (-30, 90));
}

/// Touch in the very report that brings the pen into range yields a hover sample first and then the down.
#[test]
fn touching_in_the_report_that_enters_range_hovers_first() {
    let mut s = st();
    s.feed(EV_KEY, BTN_TOOL_PEN, 1);
    s.feed(EV_KEY, BTN_TOUCH, 1);
    s.feed(EV_ABS, ABS_PRESSURE, 100);
    assert_eq!(actions(&syn(&mut s)), vec![3, 0]);
}

/// If the pen leaves range while touching, the tip is lifted and held buttons are released before the out-of-range sample.
#[test]
fn leaving_range_while_touching_lifts_and_releases_buttons_first() {
    let mut s = st();
    s.feed(EV_KEY, BTN_TOOL_PEN, 1);
    s.feed(EV_ABS, ABS_X, 1);
    syn(&mut s);
    s.feed(EV_KEY, BTN_TOUCH, 1);
    s.feed(EV_KEY, BTN_STYLUS, 1);
    s.feed(EV_KEY, BTN_STYLUS2, 1);
    assert_eq!(actions(&syn(&mut s)), vec![0, 5, 7]);
    s.feed(EV_KEY, BTN_TOOL_PEN, 0);
    assert_eq!(actions(&syn(&mut s)), vec![1, 6, 8, 4]);
    // The next approach starts from a clean slate.
    s.feed(EV_KEY, BTN_TOOL_PEN, 1);
    assert_eq!(actions(&syn(&mut s)), vec![3]);
}

/// The eraser end is flagged on the hover sample.
#[test]
fn the_eraser_end_is_flagged() {
    let mut s = st();
    s.feed(EV_KEY, BTN_TOOL_RUBBER, 1);
    s.feed(EV_ABS, ABS_X, 5);
    let out = syn(&mut s);
    assert!(out[0].eraser && out[0].action == 3);
    s.feed(EV_KEY, BTN_TOOL_RUBBER, 0);
    let out = syn(&mut s);
    assert_eq!(actions(&out), vec![4]);
}

/// Only the pen interface is selected, not the tablet's mouse-like or express-key nodes, and never our virtual devices.
#[test]
fn only_the_pen_interface_of_a_tablet_is_picked() {
    let abs = [ABS_X, ABS_Y, ABS_PRESSURE, ABS_TILT_X, ABS_TILT_Y];
    let keys = [BTN_TOOL_PEN, BTN_TOUCH, BTN_STYLUS];
    assert!(is_pen_tablet(
        "UGTABLET 6 inch PenTablet",
        0x28bd,
        &abs,
        &keys
    ));
    // The same tablet's other nodes: a mouse (relative) and a keyboard.
    assert!(!is_pen_tablet(
        "UGTABLET 6 inch PenTablet Mouse",
        0x28bd,
        &[],
        &[BTN_LEFT]
    ));
    assert!(!is_pen_tablet(
        "UGTABLET 6 inch PenTablet Keyboard",
        0x28bd,
        &[],
        &[30]
    ));
    // A touchscreen has no pen tool; our own virtual pen is never read back.
    assert!(!is_pen_tablet(
        "Touch",
        0x1,
        &[ABS_X, ABS_Y, ABS_PRESSURE],
        &[BTN_TOUCH]
    ));
    assert!(!is_pen_tablet("UScreen Pen", UINPUT_VENDOR, &abs, &keys));
}

/// `vendor:product` text parses case-insensitively and rejects empty or non-hex input.
#[test]
fn vendor_product_ids_parse() {
    assert_eq!(parse_vid_pid("28bd:0931"), Some((0x28bd, 0x0931)));
    assert_eq!(parse_vid_pid(" 056A:0357 "), Some((0x056a, 0x0357)));
    assert_eq!(parse_vid_pid(""), None);
    assert_eq!(parse_vid_pid("xx:yy"), None);
}

/// The udev rule text uses the lowercase four-digit hex ids.
#[test]
fn udev_rules_name_the_ids_in_lowercase_hex() {
    let (access, hide) = tablet_rules(0x28bd, 0x0931);
    assert!(
        access.contains("ATTRS{idVendor}==\"28bd\"")
            && access.contains("ATTRS{idProduct}==\"0931\"")
    );
    assert!(access.contains("TAG+=\"uaccess\""));
    assert!(
        hide.contains("ENV{ID_VENDOR_ID}==\"28bd\"") && hide.contains("ENV{ID_MODEL_ID}==\"0931\"")
    );
    assert!(hide.contains("ENV{LIBINPUT_IGNORE_DEVICE}=\"1\""));
    // Rule lines (not comments) are single lines, as udev requires.
    for text in [&access, &hide] {
        let rules: Vec<_> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(rules.len(), 1);
    }
}
