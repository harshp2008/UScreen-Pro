//! Unit tests of `config/mod.rs`. Moved out of the file so the code and its tests read separately; they see its private items through `use super::*`.

/// The tray toggle changes only the global stylus default and nothing else.
#[test]
fn the_tray_toggle_changes_only_the_stylus_default() {
    let mut c = FileConfig::default();
    assert!(c.set_global_stylus(true));
    assert_eq!(c.input_mode, Some(InputMode::Stylus));
    assert!(
        !c.set_global_stylus(true),
        "already Stylus: nothing to write"
    );
    assert!(c.set_global_stylus(false));
    assert_eq!(c.input_mode, Some(InputMode::Display));
    // A Touchpad default chosen separately survives the switch being turned off.
    c.input_mode = Some(InputMode::Touchpad);
    assert!(!c.set_global_stylus(false));
    assert_eq!(c.input_mode, Some(InputMode::Touchpad));
}

/// Pen button and stylus style names are trimmed, lowercased and replaced by the default when unknown.
#[test]
fn stylus_settings_are_validated() {
    assert_eq!(clean_pen_button(" Right ", "middle"), "right");
    assert_eq!(clean_pen_button("bogus", "middle"), "middle");
    assert_eq!(clean_stylus_style("TABLET"), "tablet");
    assert_eq!(clean_stylus_style("x"), "mouse");
    assert_eq!(clean_stylus_style(" Auto "), "auto");
    let mut c = FileConfig {
        stylus_button_primary: "nonsense".into(),
        ..FileConfig::default()
    };
    c.sanitize();
    assert_eq!(c.stylus_button_primary, "right");
}

use super::*;

/// Per-device encoder settings read from and written back to TOML.
#[test]
fn per_device_encoder_deserialization_and_serialization() {
    let toml_data = r#"
encoder = "h264_vaapi"
fps = 60

[tablets.DEVICE_A]
label = "Galaxy Tab S9 FE"
width = 2560
height = 1600
encoder = "hevc_vaapi"

[tablets.DEVICE_B]
label = "Old Phone"
width = 1920
height = 1080
"#;

    let cfg: FileConfig = toml::from_str(toml_data).expect("parse config");
    assert_eq!(cfg.encoder, "h264_vaapi");

    let tablet_a = cfg.tablets.get("DEVICE_A").expect("DEVICE_A profile");
    assert_eq!(tablet_a.encoder.as_deref(), Some("hevc_vaapi"));
    assert_eq!(tablet_a.width, 2560);

    let tablet_b = cfg.tablets.get("DEVICE_B").expect("DEVICE_B profile");
    assert_eq!(tablet_b.encoder, None);
    assert_eq!(tablet_b.width, 1920);

    let serialized = toml::to_string_pretty(&cfg).expect("serialize config");
    assert!(serialized.contains("encoder = \"hevc_vaapi\""));
    let re_parsed: FileConfig = toml::from_str(&serialized).expect("re-parse");
    assert_eq!(re_parsed.tablets.get("DEVICE_B").unwrap().encoder, None);
    assert_eq!(
        re_parsed
            .tablets
            .get("DEVICE_A")
            .unwrap()
            .encoder
            .as_deref(),
        Some("hevc_vaapi")
    );
}

/// Per-device profile overrides and the nickname survive serialization.
#[test]
fn device_profile_overrides_and_nickname_serialization() {
    let toml_data = r#"
encoder = "h264_vaapi"
fps = 60
quality = 18
bitrate = 20000

[tablets.TABLET_SERIAL]
label = "SM-X616B"
nickname = "My Desk Tablet"
width = 2560
height = 1600
fps = 30
quality = 16
bitrate = 35000
encoder = "hevc_vaapi"
ten_bit = true
stream_scale = 1
position = "left"
"#;

    let cfg: FileConfig = toml::from_str(toml_data).expect("parse config");
    let profile = cfg.tablets.get("TABLET_SERIAL").expect("profile exists");
    assert_eq!(profile.display_name("TABLET_SERIAL"), "My Desk Tablet");
    assert_eq!(profile.nickname.as_deref(), Some("My Desk Tablet"));
    assert_eq!(profile.fps, Some(30));
    assert_eq!(profile.quality, Some(16));
    assert_eq!(profile.bitrate, Some(35000));
    assert_eq!(profile.encoder.as_deref(), Some("hevc_vaapi"));
    assert_eq!(profile.ten_bit, Some(true));
    assert_eq!(profile.stream_scale, Some(1));
    assert_eq!(profile.position.as_deref(), Some("left"));

    let serialized = toml::to_string_pretty(&cfg).expect("serialize");
    assert!(serialized.contains("nickname = \"My Desk Tablet\""));
    assert!(serialized.contains("ten_bit = true"));
    assert!(serialized.contains("fps = 30"));

    let reparsed: FileConfig = toml::from_str(&serialized).expect("reparse");
    let p2 = reparsed.tablets.get("TABLET_SERIAL").unwrap();
    assert_eq!(p2.nickname.as_deref(), Some("My Desk Tablet"));
    assert_eq!(p2.ten_bit, Some(true));
}

/// A device's display name is its nickname; a blank nickname falls back to the model label, and with no label to the serial.
#[test]
fn device_profile_display_name_fallback_hierarchy() {
    let mut p = TabletProfile::default();
    assert_eq!(p.display_name("fallback_serial"), "fallback_serial");

    p.label = "SM-X616B".to_string();
    assert_eq!(p.display_name("fallback_serial"), "SM-X616B");

    p.nickname = Some("  ".to_string());
    assert_eq!(p.display_name("fallback_serial"), "SM-X616B");

    p.nickname = Some("Samsung Tab S9 FE".to_string());
    assert_eq!(p.display_name("fallback_serial"), "Samsung Tab S9 FE");
}

/// The keep-alive, locked-apps and Wi-Fi address settings are read from TOML.
#[test]
fn test_trio_features_config() {
    let toml_data = r#"
keep_alive = true
locked_apps = ["org.gnome.Nautilus", "google-chrome"]
wifi_address = "192.168.1.100:5555"

[devices.R52WA0A0KED]
label = "SM-X616B"
keep_alive = false
wifi_address = "192.168.1.150:5555"
locked_apps = ["com.google.android.youtube"]
"#;
    let cfg: FileConfig = toml::from_str(toml_data).expect("parse config with trio features");
    assert!(cfg.keep_alive);
    assert_eq!(cfg.wifi_address, "192.168.1.100:5555");
    assert_eq!(cfg.locked_apps, vec!["org.gnome.Nautilus", "google-chrome"]);

    let p = cfg.tablets.get("R52WA0A0KED").unwrap();
    assert_eq!(p.keep_alive, Some(false));
    assert_eq!(p.wifi_address.as_deref(), Some("192.168.1.150:5555"));
    assert_eq!(
        p.locked_apps.as_ref().unwrap(),
        &vec!["com.google.android.youtube"]
    );
}
