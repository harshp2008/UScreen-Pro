//! Unit tests of `display.rs`. Moved out of the file so the code and its tests read separately; they see its private items through `use super::*`.

use super::*;

/// Formats an EDID as the hex block that `xrandr --prop` prints.
fn hex_block(edid: &[u8]) -> String {
    edid.chunks(16)
        .map(|c| {
            format!(
                "\t\t{}\n",
                c.iter().map(|b| format!("{:02x}", b)).collect::<String>()
            )
        })
        .collect()
}

/// A fake `xrandr --prop` listing with our EDID on one output and another EDID on a second.
fn prop_output(edid_ours: &[u8], edid_real: &[u8]) -> String {
    format!(
        "Screen 0: minimum 320 x 200, current 1920 x 1080, maximum 16384 x 16384\n\
eDP-1 connected primary 1920x1080+0+0 (normal left inverted right x axis y axis) 344mm x 193mm\n\
\tEDID:\n{real}\tscaling mode: Full aspect\n\
   1920x1080     60.01*+  48.01\n\
HDMI-1 disconnected (normal left inverted right x axis y axis)\n\
DVI-I-1 connected 1280x1024+1920+0 (normal left inverted right x axis y axis) 376mm x 301mm\n\
\tEDID:\n{real}\
DVI-I-1-1 connected (normal left inverted right x axis y axis) 314mm x 195mm\n\
\tEDID:\n{ours}\tBorder: 0 0 0 0\n\
   2960x1848     90.00 +\n",
        real = hex_block(edid_real),
        ours = hex_block(edid_ours),
    )
}

/// Outputs, their geometry and their EDIDs are read from the `xrandr --prop` listing.
#[test]
fn reads_outputs_and_their_edid_from_xrandr_prop() {
    let ours = crate::display::edid::make_edid(2960, 1848, 90);
    let real = vec![0x11u8; 128];
    let o = parse_outputs(&prop_output(&ours, &real));
    assert_eq!(o.len(), 4);
    assert_eq!(o[0].geometry, Some((1920, 1080, 0, 0)));
    assert!(o[0].primary && o[0].edid == real);
    assert!(!o[1].connected);
    assert_eq!(o[3].name, "DVI-I-1-1");
    assert!(o[3].connected && o[3].geometry.is_none());
    assert_eq!(o[3].edid, ours);
}

/// Our output is found by its EDID, not by its name: the kernel's name for it can also belong to a real monitor on another GPU.
#[test]
fn finds_our_output_by_edid_not_by_name() {
    let ours = crate::display::edid::make_edid(2960, 1848, 90);
    let o = parse_outputs(&prop_output(&ours, &[0x11u8; 128]));
    // The kernel calls it DVI-I-1; so does a real monitor on the main GPU.
    let targets = vec![Target {
        name: "DVI-I-1".into(),
        edid: ours,
    }];
    let found: Vec<&str> = o
        .iter()
        .filter(|x| is_ours(x, &targets))
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(found, ["DVI-I-1-1"]);
}

/// Without an EDID the output is found by its suffixed connector name.
#[test]
fn falls_back_to_the_suffixed_name_without_an_edid() {
    let o = |name: &str| Output {
        name: name.into(),
        connected: true,
        geometry: None,
        primary: false,
        edid: Vec::new(),
    };
    let t = vec![Target {
        name: "DVI-I-2".into(),
        edid: Vec::new(),
    }];
    assert!(is_ours(&o("DVI-I-2"), &t));
    assert!(is_ours(&o("DVI-I-2-1"), &t));
    assert!(!is_ours(&o("DVI-I-2-x"), &t));
    assert!(!is_ours(&o("DVI-I-1-1"), &t));
}

/// Tablets with identical EDIDs still resolve to the connector requested for each.
#[test]
fn identical_tablet_edids_still_select_the_requested_connector() {
    let edid = crate::display::edid::make_edid(2560, 1600, 60);
    let target = vec![Target {
        name: "DVI-I-2".into(),
        edid: edid.clone(),
    }];
    for (name, expected) in [("DVI-I-1-1", false), ("DVI-I-2-2", true)] {
        let output = Output {
            name: name.into(),
            connected: true,
            geometry: Some((2560, 1600, 0, 0)),
            primary: false,
            edid: edid.clone(),
        };
        assert_eq!(is_ours(&output, &target), expected, "{name}");
    }
}

/// A name matches the device and its sub-devices, but not the next tablet (`UScreen Pen 2`).
#[test]
fn matches_a_device_and_its_sub_devices_but_not_the_next_tablet() {
    assert!(belongs_to("UScreen Pen", "UScreen Pen"));
    assert!(belongs_to("UScreen Pen Pen (0)", "UScreen Pen"));
    assert!(!belongs_to("UScreen Pen 2", "UScreen Pen"));
    assert!(!belongs_to("UScreen Pen 2 stylus", "UScreen Pen"));
    assert!(!belongs_to("UScreen Pen 3 eraser", "UScreen Pen"));
    assert!(belongs_to("UScreen Pen 2 stylus", "UScreen Pen 2"));
    assert!(!belongs_to("UScreen Pointer", "UScreen Pen"));
    assert!(belongs_to("UScreen Pen 2", "UScreen Pen 2"));
}
