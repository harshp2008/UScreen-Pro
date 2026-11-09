//! Tests of link and settings messages.

use crate::input::*;

/// The greeting sent to the tablet includes the computer's stream settings when they are known.
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
