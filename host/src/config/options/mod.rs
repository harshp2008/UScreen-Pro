//! Per-device options that follow the "global default, per-device override" rule and are kept
//! live: Touchpad mode, immersive mode, battery saver on disconnect and the idle-dim curves.
//!
//! Every tablet slot owns one [`DeviceOptions`] value in a small registry. It is recomputed from
//! `config.toml` whenever the file changes (a 500 ms mtime poll, so a GUI save reaches the tablet
//! at once without restarting anything) and whenever the tablet itself pushes a setting. The input
//! WebSocket forwards every change to the app as a `config_update` message, the daemon's gate
//! stops the video pipeline while Touchpad mode is on, and the idle task below turns the pointer
//! position into `dim_screen` / `undim_screen` messages.
//!
//! Layout:
//! * `device` - `DeviceOptions`: the effective settings of one tablet (global default plus its override) and what they imply
//! * `overrides` - applying one setting changed on the tablet to its profile in config.toml
//! * `registry` - the live per-tablet registry: slots, watchers that republish options when config.toml changes, and the accessors the rest of the host uses
//! * `idle` - the idle-dim state machine that fades the tablet's screen when it is not being used

mod device;
mod idle;
mod overrides;
mod registry;
pub use device::*;
pub use idle::*;
use overrides::*;
pub use registry::*;

use crate::config::{resolve_curve, FileConfig, InputMode, TabletProfile};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{watch, Notify};
use tracing::{info, warn};

/// Mode forced from the command line (`--pen-only`); wins over the global config value but not
/// over a device's own override.
static CLI_MODE: OnceLock<InputMode> = OnceLock::new();

/// Make the command line's mode the global default for this run.
pub fn set_cli_mode(mode: InputMode) {
    let _ = CLI_MODE.set(mode);
}

/// Tests for resolving global and per-device settings into `DeviceOptions`, and for the idle-dim timer.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// A per-device override wins; anything it leaves unset comes from the global settings.
    #[test]
    fn device_override_wins_and_global_fills_the_rest() {
        let cfg = FileConfig {
            input_mode: Some(InputMode::Display),
            idle_dim_enabled: true,
            idle_dim_timeout_secs: 90,
            ..FileConfig::default()
        };
        let p = TabletProfile {
            input_mode: Some(InputMode::Touchpad),
            idle_dim_timeout_secs: Some(20),
            dim_curve: Some("linear".into()),
            dim_duration_ms: Some(5000),
            undim_curve: Some("exponential".into()),
            undim_duration_ms: Some(500),
            ..TabletProfile::default()
        };
        let o = DeviceOptions::resolve(&cfg, Some(&p));
        assert_eq!(o.input_mode, InputMode::Touchpad);
        assert!(o.idle_dim_enabled, "inherited from the global value");
        assert_eq!(o.idle_dim_timeout_secs, 20);
        assert_eq!((o.dim_curve.as_str(), o.dim_duration_ms), ("linear", 5000));
        assert_eq!(
            (o.undim_curve.as_str(), o.undim_duration_ms),
            ("exponential", 500)
        );
        let g = DeviceOptions::resolve(&cfg, None);
        assert_eq!(g.input_mode, InputMode::Display);
        assert_eq!(g.idle_dim_timeout_secs, 90);
    }

    /// Out-of-range profile values are clamped to their limits; an unknown dim curve becomes `ease_out` and an unknown orientation becomes `auto`.
    #[test]
    fn bad_profile_values_are_clamped() {
        let p = TabletProfile {
            idle_dim_timeout_secs: Some(1),
            dim_duration_ms: Some(10_000_000),
            dim_curve: Some("wobble".into()),
            orientation: Some("sideways".into()),
            ..TabletProfile::default()
        };
        let o = DeviceOptions::resolve(&FileConfig::default(), Some(&p));
        assert_eq!(
            o.idle_dim_timeout_secs,
            crate::config::MIN_IDLE_TIMEOUT_SECS
        );
        assert_eq!(o.dim_duration_ms, crate::config::MAX_FADE_MS);
        assert_eq!(o.dim_curve, "ease_out");
        assert_eq!(o.orientation, "auto");
    }

    /// Saved and custom dim curves reach the tablet as cubic-Bezier strings.
    #[test]
    fn saved_and_custom_curves_reach_the_tablet_as_bezier_strings() {
        let cfg = FileConfig {
            curve_presets: vec![crate::config::CurvePreset {
                name: "Snappy".into(),
                x1: 0.2,
                y1: 0.9,
                x2: 0.3,
                y2: 1.0,
            }],
            dim_curve: "preset:Snappy".into(),
            undim_curve: "bezier:0.25,0.1,0.25,1".into(),
            ..FileConfig::default()
        };
        let o = DeviceOptions::resolve(&cfg, None);
        assert_eq!(o.dim_curve, "bezier:0.2,0.9,0.3,1");
        assert_eq!(o.undim_curve, "bezier:0.25,0.1,0.25,1");
        // A deleted preset falls back to the default instead of breaking the fade.
        let gone = FileConfig {
            dim_curve: "preset:Missing".into(),
            ..FileConfig::default()
        };
        assert_eq!(DeviceOptions::resolve(&gone, None).dim_curve, "ease_out");
    }

    /// Settings sent from the tablet are stored as device overrides and report whether anything changed.
    #[test]
    fn tablet_settings_become_device_overrides() {
        let mut p = TabletProfile::default();
        assert_eq!(
            set_override(&mut p, "input_mode", &json!("touchpad")),
            Ok(true)
        );
        assert_eq!(
            set_override(&mut p, "input_mode", &json!("touchpad")),
            Ok(false)
        );
        assert_eq!(p.input_mode, Some(InputMode::Touchpad));
        assert_eq!(set_override(&mut p, "show_stats", &json!(true)), Ok(true));
        assert_eq!(
            set_override(&mut p, "orientation", &json!("camera_up")),
            Ok(true)
        );
        assert!(set_override(&mut p, "input_mode", &json!("flying")).is_err());
        assert!(set_override(&mut p, "show_stats", &json!("yes")).is_err());
        assert!(set_override(&mut p, "nonsense", &json!(true)).is_err());
    }

    /// The old `pen_only` and `touchpad_mode` flags migrate to `input_mode` when the config is loaded.
    #[test]
    fn legacy_flags_migrate_to_input_mode() {
        let mut cfg: FileConfig =
            toml::from_str("pen_only = true\n[tablets.A]\ntouchpad_mode = true\n").unwrap();
        cfg.sanitize();
        assert_eq!(cfg.input_mode, Some(InputMode::Stylus));
        assert_eq!(cfg.tablets["A"].input_mode, Some(InputMode::Touchpad));
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("input_mode = \"stylus\""));
        assert!(!text.contains("touchpad_mode"));
        // Once present, a stale `pen_only` cannot win over an explicit choice.
        let mut again: FileConfig =
            toml::from_str("pen_only = true\ninput_mode = \"display\"\n").unwrap();
        again.sanitize();
        assert_eq!(again.input_mode, Some(InputMode::Display));
    }

    /// The config update sent to a tablet carries every option key, so the tablet never keeps a stale value.
    #[test]
    fn config_update_carries_every_option() {
        let v = DeviceOptions::resolve(&FileConfig::default(), None).config_update();
        for k in [
            "input_mode",
            "immersive_mode",
            "show_stats",
            "touch_input",
            "touch_on_lock_screen",
            "keep_display_alive_when_locked",
            "keep_connected_in_background",
            "blank_pen_only",
            "orientation",
            "theme_mode",
            "battery_saver_on_disconnect",
            "battery_saver_lock",
            "idle_dim_enabled",
            "idle_dim_timeout_secs",
            "dim_curve",
            "undim_curve",
            "dim_duration_ms",
            "undim_duration_ms",
            "touchpad_haptics",
            "right_click_method",
            "native_gestures",
            "touchpad_sensitivity",
            "touchpad_shape",
            "stylus_style",
            "stylus_button_primary",
            "stylus_button_secondary",
            "wifi_configured",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["type"], "config_update");
        assert_eq!(v["input_mode"], "display");
    }

    /// After the idle timeout the screen dims, and it un-dims when the pointer returns.
    #[tokio::test]
    async fn dims_after_the_timeout_and_undims_when_the_pointer_returns() {
        let o = DeviceOptions {
            idle_dim_enabled: true,
            idle_dim_timeout_secs: 1,
            dim_curve: "linear".into(),
            dim_duration_ms: 5000,
            undim_curve: "exponential".into(),
            undim_duration_ms: 500,
            ..DeviceOptions::resolve(&FileConfig::default(), None)
        };
        let (_opts_tx, opts_rx) = watch::channel(o);
        let (pointer_tx, pointer_rx) = watch::channel(false);
        let sent = Arc::new(StdMutex::new(Vec::<Value>::new()));
        let sink = sent.clone();
        tokio::spawn(idle_loop(
            opts_rx,
            pointer_rx,
            Arc::new(Notify::new()),
            move |m| sink.lock().unwrap().push(m),
        ));
        tokio::time::sleep(Duration::from_millis(1400)).await;
        {
            let got = sent.lock().unwrap();
            assert_eq!(got.len(), 1);
            assert_eq!(got[0]["type"], "dim_screen");
            assert_eq!(got[0]["duration"], 5000);
            assert_eq!(got[0]["curve"], "linear");
        }
        pointer_tx.send(true).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let got = sent.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1]["type"], "undim_screen");
        assert_eq!(got[1]["duration"], 500);
        assert_eq!(got[1]["curve"], "exponential");
    }

    /// Activity restarts the idle timer and wakes a dimmed tablet.
    #[tokio::test]
    async fn activity_restarts_the_timer_and_wakes_a_dimmed_tablet() {
        let o = DeviceOptions {
            idle_dim_enabled: true,
            idle_dim_timeout_secs: 1,
            ..DeviceOptions::resolve(&FileConfig::default(), None)
        };
        let (_opts_tx, opts_rx) = watch::channel(o);
        let (_pointer_tx, pointer_rx) = watch::channel(false);
        let activity = Arc::new(Notify::new());
        let sent = Arc::new(StdMutex::new(Vec::<Value>::new()));
        let sink = sent.clone();
        tokio::spawn(idle_loop(opts_rx, pointer_rx, activity.clone(), move |m| {
            sink.lock().unwrap().push(m)
        }));
        tokio::time::sleep(Duration::from_millis(700)).await;
        activity.notify_one();
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert!(
            sent.lock().unwrap().is_empty(),
            "timer restarted by activity"
        );
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(
            sent.lock().unwrap().len(),
            1,
            "dimmed after a full quiet second"
        );
        activity.notify_one();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(sent.lock().unwrap()[1]["type"], "undim_screen");
    }

    /// No dimming while the pointer is on the tablet or when the feature is off.
    #[tokio::test]
    async fn never_dims_while_the_pointer_is_on_the_tablet_or_the_feature_is_off() {
        let on = DeviceOptions {
            idle_dim_enabled: true,
            idle_dim_timeout_secs: 1,
            ..DeviceOptions::resolve(&FileConfig::default(), None)
        };
        let (_a, opts_rx) = watch::channel(on.clone());
        let (_b, pointer_rx) = watch::channel(true);
        let sent = Arc::new(StdMutex::new(Vec::<Value>::new()));
        let sink = sent.clone();
        tokio::spawn(idle_loop(
            opts_rx,
            pointer_rx,
            Arc::new(Notify::new()),
            move |m| sink.lock().unwrap().push(m),
        ));
        let off = DeviceOptions {
            idle_dim_enabled: false,
            ..on
        };
        let (_c, opts_rx2) = watch::channel(off);
        let (_d, pointer_rx2) = watch::channel(false);
        let sink2 = sent.clone();
        tokio::spawn(idle_loop(
            opts_rx2,
            pointer_rx2,
            Arc::new(Notify::new()),
            move |m| sink2.lock().unwrap().push(m),
        ));
        tokio::time::sleep(Duration::from_millis(1400)).await;
        assert!(sent.lock().unwrap().is_empty());
    }
    /// Touchpad and stylus modes have no pointer-crossing signal, so they dim on inactivity alone.
    #[tokio::test]
    async fn touchpad_and_stylus_dim_on_inactivity_alone() {
        let o = DeviceOptions {
            input_mode: InputMode::Touchpad,
            idle_dim_enabled: true,
            idle_dim_timeout_secs: 1,
            ..DeviceOptions::resolve(&FileConfig::default(), None)
        };
        let (_a, opts_rx) = watch::channel(o);
        // The pointer flag still says "on the tablet" (no helper runs outside Display mode).
        let (_b, pointer_rx) = watch::channel(true);
        let sent = Arc::new(StdMutex::new(Vec::<Value>::new()));
        let sink = sent.clone();
        tokio::spawn(idle_loop(
            opts_rx,
            pointer_rx,
            Arc::new(Notify::new()),
            move |m| sink.lock().unwrap().push(m),
        ));
        tokio::time::sleep(Duration::from_millis(1400)).await;
        assert_eq!(sent.lock().unwrap().len(), 1);
    }

    /// The pen menu settings follow the device override and are included in the update sent to the tablet.
    #[test]
    fn the_pen_menu_settings_follow_the_device_override_and_reach_the_tablet() {
        let cfg = FileConfig::default();
        let p = TabletProfile {
            pen_menu: Some(false),
            pen_menu_hold_ms: Some(900),
            pen_menu_style: Some("icons".into()),
            stylus_idle_hide_timeout: Some(15),
            ..TabletProfile::default()
        };
        let o = DeviceOptions::resolve(&cfg, Some(&p));
        assert!(!o.pen_menu && o.pen_menu_hold_ms == 900 && o.pen_menu_style == "icons");
        let m = o.config_update();
        assert_eq!(m["pen_menu"], false);
        assert_eq!(m["pen_menu_hold_ms"], 900);
        assert_eq!(m["pen_menu_style"], "icons");
        assert_eq!(m["stylus_idle_hide_timeout"], 15);
        let g = DeviceOptions::resolve(&cfg, None);
        assert!(g.pen_menu, "another device keeps the global value");
    }
}
