//! Per-device options that follow the "global default, per-device override" rule and are kept
//! live: Touchpad mode, immersive mode, battery saver on disconnect and the idle-dim curves.
//!
//! Every tablet slot owns one [`DeviceOptions`] value in a small registry. It is recomputed from
//! `config.toml` whenever the file changes (a 500 ms mtime poll, so a GUI save reaches the tablet
//! at once without restarting anything) and whenever the tablet itself pushes a setting. The input
//! WebSocket forwards every change to the app as a `config_update` message, the daemon's gate
//! stops the video pipeline while Touchpad mode is on, and the idle task below turns the pointer
//! position into `dim_screen` / `undim_screen` messages.

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

/// The effective (global merged with the device's override) value of every live option.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceOptions {
    pub input_mode: InputMode,
    pub immersive_mode: bool,
    pub show_stats: bool,
    pub touch_input: bool,
    pub touch_on_lock_screen: bool,
    pub keep_display_alive_when_locked: bool,
    pub keep_connected_in_background: bool,
    pub blank_pen_only: bool,
    pub orientation: String,
    pub theme_mode: String,
    pub battery_saver_on_disconnect: bool,
    pub battery_saver_lock: bool,
    pub idle_dim_enabled: bool,
    pub idle_dim_timeout_secs: u32,
    /// Wire form: a preset name or `bezier:x1,y1,x2,y2`.
    pub dim_curve: String,
    pub undim_curve: String,
    pub dim_duration_ms: u32,
    pub undim_duration_ms: u32,
    pub touchpad_haptics: bool,
    /// `bottom_right_zone` or `two_finger_tap`.
    pub right_click_method: String,
    /// `full_screen`, `standard_rectangle` or `wide_rectangle`.
    pub touchpad_shape: String,
    /// What the pen's two buttons do (see `config::PEN_BUTTON_ACTIONS`).
    pub stylus_button_primary: String,
    pub stylus_button_secondary: String,
    /// Informational: a Wi-Fi address is configured for this device, so the app hides its
    /// "Setup Wireless Pairing" button.
    pub wifi_configured: bool,
}

impl DeviceOptions {
    /// Merge `profile`'s overrides over the global values in `cfg`, clamping what a hand-edited
    /// profile could get wrong.
    pub fn resolve(cfg: &FileConfig, profile: Option<&TabletProfile>) -> Self {
        let default = TabletProfile::default();
        let p = profile.unwrap_or(&default);
        macro_rules! pick {
            ($f:ident) => {
                p.$f.clone().unwrap_or_else(|| cfg.$f.clone())
            };
        }
        let mut g = cfg.clone();
        g.immersive_mode = pick!(immersive_mode);
        g.show_stats = pick!(show_stats);
        g.touch_input = pick!(touch_input);
        g.touch_on_lock_screen = pick!(touch_on_lock_screen);
        g.keep_display_alive_when_locked = pick!(keep_display_alive_when_locked);
        g.keep_connected_in_background = pick!(keep_connected_in_background);
        g.blank_pen_only = pick!(blank_pen_only);
        g.orientation = pick!(orientation);
        g.theme_mode = pick!(theme_mode);
        g.battery_saver_on_disconnect = pick!(battery_saver_on_disconnect);
        g.battery_saver_lock = pick!(battery_saver_lock);
        g.idle_dim_enabled = pick!(idle_dim_enabled);
        g.idle_dim_timeout_secs = pick!(idle_dim_timeout_secs);
        g.dim_curve = pick!(dim_curve);
        g.undim_curve = pick!(undim_curve);
        g.dim_duration_ms = pick!(dim_duration_ms);
        g.undim_duration_ms = pick!(undim_duration_ms);
        g.touchpad_haptics = pick!(touchpad_haptics);
        g.right_click_method = pick!(right_click_method);
        g.touchpad_shape = pick!(touchpad_shape);
        g.stylus_button_primary = pick!(stylus_button_primary);
        g.stylus_button_secondary = pick!(stylus_button_secondary);
        g.sanitize();
        let global_mode = CLI_MODE
            .get()
            .copied()
            .or(cfg.input_mode)
            .unwrap_or_default();
        let wifi = |s: &Option<String>| s.as_deref().is_some_and(|a| !a.trim().is_empty());
        Self {
            input_mode: p.input_mode.unwrap_or(global_mode),
            immersive_mode: g.immersive_mode,
            show_stats: g.show_stats,
            touch_input: g.touch_input,
            touch_on_lock_screen: g.touch_on_lock_screen,
            keep_display_alive_when_locked: g.keep_display_alive_when_locked,
            keep_connected_in_background: g.keep_connected_in_background,
            blank_pen_only: g.blank_pen_only,
            orientation: g.orientation.clone(),
            theme_mode: g.theme_mode.clone(),
            battery_saver_on_disconnect: g.battery_saver_on_disconnect,
            battery_saver_lock: g.battery_saver_lock,
            idle_dim_enabled: g.idle_dim_enabled,
            idle_dim_timeout_secs: g.idle_dim_timeout_secs,
            dim_curve: resolve_curve(&g.dim_curve, &cfg.curve_presets, "ease_out"),
            undim_curve: resolve_curve(&g.undim_curve, &cfg.curve_presets, "ease_in"),
            dim_duration_ms: g.dim_duration_ms,
            undim_duration_ms: g.undim_duration_ms,
            touchpad_haptics: g.touchpad_haptics,
            right_click_method: g.right_click_method.clone(),
            touchpad_shape: g.touchpad_shape.clone(),
            stylus_button_primary: g.stylus_button_primary.clone(),
            stylus_button_secondary: g.stylus_button_secondary.clone(),
            wifi_configured: wifi(&p.wifi_address) || !cfg.wifi_address.trim().is_empty(),
        }
    }

    /// The `config_update` message the tablet applies to its own preferences and UI.
    pub fn config_update(&self) -> Value {
        json!({
            "type": "config_update",
            "input_mode": self.input_mode.as_str(),
            "immersive_mode": self.immersive_mode,
            "show_stats": self.show_stats,
            "touch_input": self.touch_input,
            "touch_on_lock_screen": self.touch_on_lock_screen,
            "keep_display_alive_when_locked": self.keep_display_alive_when_locked,
            "keep_connected_in_background": self.keep_connected_in_background,
            "blank_pen_only": self.blank_pen_only,
            "orientation": self.orientation,
            "theme_mode": self.theme_mode,
            "battery_saver_on_disconnect": self.battery_saver_on_disconnect,
            "battery_saver_lock": self.battery_saver_lock,
            "idle_dim_enabled": self.idle_dim_enabled,
            "idle_dim_timeout_secs": self.idle_dim_timeout_secs,
            "dim_curve": self.dim_curve,
            "undim_curve": self.undim_curve,
            "dim_duration_ms": self.dim_duration_ms,
            "undim_duration_ms": self.undim_duration_ms,
            "touchpad_haptics": self.touchpad_haptics,
            "right_click_method": self.right_click_method,
            "touchpad_shape": self.touchpad_shape,
            "stylus_button_primary": self.stylus_button_primary,
            "stylus_button_secondary": self.stylus_button_secondary,
            "wifi_configured": self.wifi_configured,
        })
    }

    fn dim_message(&self) -> Value {
        json!({ "type": "dim_screen", "duration": self.dim_duration_ms, "curve": self.dim_curve })
    }

    fn undim_message(&self) -> Value {
        json!({ "type": "undim_screen", "duration": self.undim_duration_ms, "curve": self.undim_curve })
    }
}

/// Store one setting pushed by the tablet as the device's override. `Ok(true)` when it changed.
fn set_override(p: &mut TabletProfile, key: &str, v: &Value) -> Result<bool, ()> {
    fn set<T: PartialEq>(slot: &mut Option<T>, new: T) -> bool {
        if slot.as_ref() == Some(&new) {
            false
        } else {
            *slot = Some(new);
            true
        }
    }
    let b = v.as_bool().ok_or(());
    let n = v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or(());
    let s = v.as_str().map(str::to_string).ok_or(());
    Ok(match key {
        "input_mode" => {
            let mode = match s?.as_str() {
                "display" => InputMode::Display,
                "touchpad" => InputMode::Touchpad,
                "stylus" => InputMode::Stylus,
                _ => return Err(()),
            };
            set(&mut p.input_mode, mode)
        }
        "immersive_mode" => set(&mut p.immersive_mode, b?),
        "show_stats" => set(&mut p.show_stats, b?),
        "touch_input" => set(&mut p.touch_input, b?),
        "touch_on_lock_screen" => set(&mut p.touch_on_lock_screen, b?),
        "keep_display_alive_when_locked" => set(&mut p.keep_display_alive_when_locked, b?),
        "keep_connected_in_background" => set(&mut p.keep_connected_in_background, b?),
        "blank_pen_only" => set(&mut p.blank_pen_only, b?),
        "orientation" => set(&mut p.orientation, s?),
        "theme_mode" => set(&mut p.theme_mode, s?),
        "battery_saver_on_disconnect" => set(&mut p.battery_saver_on_disconnect, b?),
        "battery_saver_lock" => set(&mut p.battery_saver_lock, b?),
        "idle_dim_enabled" => set(&mut p.idle_dim_enabled, b?),
        "idle_dim_timeout_secs" => set(&mut p.idle_dim_timeout_secs, n?),
        "touchpad_haptics" => set(&mut p.touchpad_haptics, b?),
        "stylus_button_primary" | "stylus_button_secondary" => {
            let v = s?;
            if !crate::config::PEN_BUTTON_ACTIONS.contains(&v.as_str()) {
                return Err(());
            }
            if key == "stylus_button_primary" {
                set(&mut p.stylus_button_primary, v)
            } else {
                set(&mut p.stylus_button_secondary, v)
            }
        }
        "touchpad_shape" => match s?.as_str() {
            m @ ("full_screen" | "standard_rectangle" | "wide_rectangle") => {
                set(&mut p.touchpad_shape, m.to_string())
            }
            _ => return Err(()),
        },
        "right_click_method" => match s?.as_str() {
            m @ ("bottom_right_zone" | "two_finger_tap") => {
                set(&mut p.right_click_method, m.to_string())
            }
            _ => return Err(()),
        },
        _ => return Err(()),
    })
}

struct Slot {
    /// Profile keys of the device in this slot (hardware serial, adb serial), best first.
    keys: Vec<String>,
    tx: watch::Sender<DeviceOptions>,
    /// Derived from `tx`: the device is in Stylus mode (what the input mapping calls "pen-only").
    pen_only: watch::Sender<bool>,
    /// Whether the desktop pointer is on this tablet's screen. `true` until the helper says
    /// otherwise, so a missing signal can never dim a tablet.
    pointer: watch::Sender<bool>,
    /// Pinged by every touch / pen / touchpad event from the tablet.
    activity: Arc<Notify>,
    idle_task: bool,
}

fn slots() -> &'static Mutex<HashMap<u32, Slot>> {
    static SLOTS: OnceLock<Mutex<HashMap<u32, Slot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_slot<R>(instance: u32, f: impl FnOnce(&mut Slot) -> R) -> R {
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    let slot = map.entry(instance).or_insert_with(|| {
        let first = DeviceOptions::resolve(&FileConfig::load(), None);
        Slot {
            keys: Vec::new(),
            pen_only: watch::channel(first.input_mode == InputMode::Stylus).0,
            tx: watch::channel(first).0,
            pointer: watch::channel(true).0,
            activity: Arc::new(Notify::new()),
            idle_task: false,
        }
    });
    f(slot)
}

/// Live options of a slot.
pub fn options_rx(instance: u32) -> watch::Receiver<DeviceOptions> {
    with_slot(instance, |s| s.tx.subscribe())
}

/// Whether the slot's device is in Stylus mode, live.
pub fn pen_only_rx(instance: u32) -> watch::Receiver<bool> {
    with_slot(instance, |s| s.pen_only.subscribe())
}

/// What the slot's pen buttons do right now: (side button, second button).
pub fn current_pen_buttons(instance: u32) -> (String, String) {
    with_slot(instance, |s| {
        let o = s.tx.borrow();
        (
            o.stylus_button_primary.clone(),
            o.stylus_button_secondary.clone(),
        )
    })
}

/// The slot's current input mode.
pub fn current_mode(instance: u32) -> InputMode {
    with_slot(instance, |s| s.tx.borrow().input_mode)
}

fn profile_of<'a>(cfg: &'a FileConfig, keys: &[String]) -> Option<&'a TabletProfile> {
    keys.iter().find_map(|k| cfg.tablets.get(k))
}

fn publish(slot: &mut Slot, cfg: &FileConfig) {
    let next = DeviceOptions::resolve(cfg, profile_of(cfg, &slot.keys));
    let stylus = next.input_mode == InputMode::Stylus;
    slot.tx.send_if_modified(|cur| {
        if *cur == next {
            false
        } else {
            *cur = next;
            true
        }
    });
    slot.pen_only.send_if_modified(|cur| {
        if *cur == stylus {
            false
        } else {
            *cur = stylus;
            true
        }
    });
}

/// Tell the registry which device sits in `instance`, so its profile is the one merged over the
/// global values from now on.
pub fn bind(instance: u32, keys: &[&str]) {
    let cfg = FileConfig::load();
    with_slot(instance, |s| {
        s.keys = keys
            .iter()
            .filter(|k| !k.is_empty())
            .map(|k| k.to_string())
            .collect();
        publish(s, &cfg);
    });
}

/// Recompute every slot from `config.toml`.
pub fn refresh_all() {
    let cfg = FileConfig::load();
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    for slot in map.values_mut() {
        publish(slot, &cfg);
    }
}

/// Watch `config.toml` and push changes into every slot. Cheap: one `stat` every 500 ms.
pub fn spawn_config_watcher() {
    tokio::spawn(async {
        let mtime = || {
            std::fs::metadata(crate::config::config_path())
                .and_then(|m| m.modified())
                .ok()
        };
        let mut last = mtime();
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let now = mtime();
            if now != last {
                last = now;
                refresh_all();
            }
        }
    });
}

/// A setting changed on the tablet itself: store it as that device's override. Returns whether
/// anything changed. Unknown keys, bad values and unpaired devices are ignored.
pub fn set_from_device(instance: u32, key: &str, value: &Value) -> bool {
    let keys = with_slot(instance, |s| s.keys.clone());
    let mut cfg = FileConfig::load();
    let Some(profile_key) = keys
        .iter()
        .find(|k| cfg.tablets.contains_key(*k))
        .cloned()
        .or_else(|| {
            cfg.auto_pair_new_devices
                .then(|| keys.first().cloned())
                .flatten()
        })
    else {
        warn!("Tablet setting {} ignored: this device has no profile", key);
        return false;
    };
    let p = cfg.tablets.entry(profile_key).or_default();
    match set_override(p, key, value) {
        Ok(true) => {}
        Ok(false) => return false,
        Err(()) => {
            warn!("Tablet setting {}={} rejected", key, value);
            return false;
        }
    }
    if cfg.save().is_err() {
        return false;
    }
    info!("Tablet changed {} to {}", key, value);
    refresh_all();
    true
}

/// The helper reported whether the desktop pointer is on this tablet's screen.
pub fn set_pointer(instance: u32, on_tablet: bool) {
    with_slot(instance, |s| {
        s.pointer.send_if_modified(|cur| {
            if *cur == on_tablet {
                false
            } else {
                *cur = on_tablet;
                true
            }
        });
    });
}

/// A touch, pen or touchpad event arrived from this slot's tablet.
pub fn note_activity(instance: u32) {
    let n = with_slot(instance, |s| s.activity.clone());
    n.notify_one();
}

/// Start the idle-dim task of a slot (once). It dims the tablet after the pointer has been away
/// and the tablet unused for the configured time, and undims it when either comes back.
pub fn spawn_idle_task(instance: u32) {
    let first = with_slot(instance, |s| !std::mem::replace(&mut s.idle_task, true));
    if !first {
        return;
    }
    let opts = options_rx(instance);
    let (pointer, activity) = with_slot(instance, |s| (s.pointer.subscribe(), s.activity.clone()));
    tokio::spawn(idle_loop(opts, pointer, activity, move |msg| {
        crate::net::notices::send(instance, msg)
    }));
}

async fn idle_loop(
    mut opts_rx: watch::Receiver<DeviceOptions>,
    mut pointer_rx: watch::Receiver<bool>,
    activity: Arc<Notify>,
    send: impl Fn(Value),
) {
    let mut dimmed = false;
    loop {
        let o = opts_rx.borrow().clone();
        // Without Display mode there is no virtual screen for the pointer to be on, so it is always
        // "away" and only the tablet's own input keeps the timer from running out.
        let pointer_away = !*pointer_rx.borrow() || o.input_mode != InputMode::Display;
        // Touchpad mode has no screen for the pointer to be on; there the timer is simply "no
        // input from the tablet for the timeout".
        let armed = o.idle_dim_enabled && pointer_away;
        if !armed {
            if dimmed {
                dimmed = false;
                send(o.undim_message());
            }
            tokio::select! {
                r = opts_rx.changed() => if r.is_err() { return },
                r = pointer_rx.changed() => if r.is_err() { return },
            }
            continue;
        }
        if dimmed {
            tokio::select! {
                _ = activity.notified() => {
                    dimmed = false;
                    send(o.undim_message());
                }
                r = opts_rx.changed() => if r.is_err() { return },
                r = pointer_rx.changed() => if r.is_err() { return },
            }
            continue;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(o.idle_dim_timeout_secs as u64)) => {
                dimmed = true;
                send(o.dim_message());
            }
            _ = activity.notified() => {}
            r = opts_rx.changed() => if r.is_err() { return },
            r = pointer_rx.changed() => if r.is_err() { return },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

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
            "touchpad_shape",
            "stylus_button_primary",
            "stylus_button_secondary",
            "wifi_configured",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["type"], "config_update");
        assert_eq!(v["input_mode"], "display");
    }

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
}
