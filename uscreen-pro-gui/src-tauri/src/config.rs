//! Persistent configuration (`~/.config/uscreen/config.toml`) shared with the host daemon.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A fade curve saved by the user: a CSS-style cubic Bezier through (0,0) and (1,1).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CurvePreset {
    pub name: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Per-device section (`[tablets.<serial>]`) of config.toml; mirrors the host's `TabletProfile`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TabletProfile {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub width: u32,
    pub height: u32,
    pub width_mm: u32,
    pub height_mm: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_scale: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ten_bit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_apps: Option<Vec<String>>,
    /// Input mode of this device (`display`, `touchpad`, `stylus`); unset inherits the global one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_mode: Option<String>,
    /// Per-device overrides of the global power, input and tablet-app settings of the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub immersive_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_stats: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_input: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_on_lock_screen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_display_alive_when_locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_connected_in_background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blank_pen_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_saver_on_disconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_saver_lock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_dim_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_dim_timeout_secs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim_curve: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undim_curve: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim_duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undim_duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchpad_haptics: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_gestures: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchpad_sensitivity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_click_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchpad_shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylus_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylus_button_primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylus_button_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<String>,
    /// `Some(false)` when the user skipped wireless setup; `None` (older profiles) counts as configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wireless_configured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_level: Option<u32>,
    #[serde(flatten)]
    pub extra: toml::value::Table,
}

/// The GUI's view of config.toml: global stream settings plus the per-device profiles. Unknown keys are tolerated so host-side additions never break the GUI.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub encoder: String,
    pub fps: u32,
    pub bitrate: u32,
    pub width: u32,
    pub height: u32,
    pub quality: u32,
    pub stream_scale: u32,
    pub pen_only: bool,
    pub position: String,
    pub ten_bit: bool,
    pub require_token: bool,
    pub check_updates: bool,
    pub max_tablets: u32,
    pub wifi_address: String,
    pub vaapi_device: String,
    pub pointer_handoff: bool,
    pub auto_resolution: bool,
    pub video_port: u16,
    pub input_port: u16,
    pub auto_launch_app: bool,
    pub max_performance_mode: bool,
    pub hevc_low_latency_tuning: bool,
    #[serde(default)]
    pub software_cursor: bool,
    pub keep_alive: bool,
    #[serde(default)]
    pub locked_apps: Vec<String>,
    pub allow_legacy_fallback: bool,
    pub auto_enable_wifi_adb: bool,
    /// Global default input mode: `display`, `touchpad` or `stylus`. Empty in files from before the
    /// enum; `load()` fills it from the legacy `pen_only` / `touchpad_mode` flags.
    pub input_mode: String,
    /// Global defaults of the power, input and tablet-app settings; a device may override each one.
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
    pub dim_curve: String,
    pub undim_curve: String,
    pub dim_duration_ms: u32,
    pub undim_duration_ms: u32,
    /// Light vibration on touchpad clicks.
    pub touchpad_haptics: bool,
    /// Use native gestures (Wayland / libinput): the tablet's fingers are replayed on a virtual touchpad.
    pub native_gestures: bool,
    /// Touchpad sensitivity in percent (50-200).
    pub touchpad_sensitivity: u32,
    /// `bottom_right_zone` or `two_finger_tap`.
    pub right_click_method: String,
    /// `full_screen`, `standard_rectangle` or `wide_rectangle`.
    pub touchpad_shape: String,
    /// What the pen does in Stylus mode: `mouse` or `tablet`.
    pub stylus_style: String,
    /// First pen side button in `mouse` style: `right`, `middle` or `none`.
    pub stylus_button_primary: String,
    /// Second pen side button in `mouse` style.
    pub stylus_button_secondary: String,
    /// Fade curves saved in the curve editor (usable as `preset:<name>`).
    pub curve_presets: Vec<CurvePreset>,
    pub version: String,
    #[serde(default, alias = "devices")]
    pub tablets: HashMap<String, TabletProfile>,
    #[serde(flatten)]
    pub extra: toml::value::Table,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            encoder: "h264_nvenc".into(),
            fps: 60,
            bitrate: 20000,
            width: 2960,
            height: 1848,
            quality: 18,
            stream_scale: 1,
            pen_only: false,
            position: "right".into(),
            ten_bit: false,
            require_token: true,
            check_updates: true,
            max_tablets: 1,
            wifi_address: String::new(),
            vaapi_device: String::new(),
            pointer_handoff: true,
            auto_resolution: true,
            video_port: 8890,
            input_port: 8891,
            auto_launch_app: true,
            max_performance_mode: false,
            hevc_low_latency_tuning: true,
            software_cursor: false,
            keep_alive: false,
            locked_apps: Vec::new(),
            allow_legacy_fallback: false,
            auto_enable_wifi_adb: true,
            input_mode: "display".into(),
            immersive_mode: false,
            show_stats: false,
            touch_input: true,
            touch_on_lock_screen: false,
            keep_display_alive_when_locked: true,
            keep_connected_in_background: false,
            blank_pen_only: true,
            orientation: "auto".into(),
            theme_mode: "system".into(),
            battery_saver_on_disconnect: false,
            battery_saver_lock: false,
            idle_dim_enabled: false,
            idle_dim_timeout_secs: 60,
            dim_curve: "ease_out".into(),
            undim_curve: "ease_in".into(),
            dim_duration_ms: 3000,
            undim_duration_ms: 500,
            touchpad_haptics: true,
            native_gestures: true,
            touchpad_sensitivity: 100,
            right_click_method: "bottom_right_zone".into(),
            touchpad_shape: "full_screen".into(),
            stylus_style: "mouse".into(),
            stylus_button_primary: "right".into(),
            stylus_button_secondary: "middle".into(),
            curve_presets: Vec::new(),
            version: env!("GIT_VERSION_STRING").to_string(),
            tablets: HashMap::new(),
            extra: toml::value::Table::new(),
        }
    }
}

/// The user's home directory (`$HOME`), or an empty path if unset.
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Same file the host daemon reads: `~/.config/uscreen/config.toml`.
pub fn config_path() -> PathBuf {
    home_dir().join(".config/uscreen/config.toml")
}

/// Read the config, falling back to defaults when missing or unparsable.
pub async fn load() -> AppConfig {
    let mut cfg: AppConfig = match tokio::fs::read_to_string(config_path()).await {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    };
    migrate_input_mode(&mut cfg);
    cfg
}

/// Fill `input_mode` from the two booleans it replaced (`pen_only`, and `touchpad_mode` kept in the
/// unknown-keys table), the same one-time migration the daemon does.
fn migrate_input_mode(cfg: &mut AppConfig) {
    let legacy_touchpad = |extra: &mut toml::value::Table| {
        extra
            .remove("touchpad_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let global_touchpad = legacy_touchpad(&mut cfg.extra);
    if !matches!(cfg.input_mode.as_str(), "display" | "touchpad" | "stylus") {
        cfg.input_mode = if global_touchpad {
            "touchpad"
        } else if cfg.pen_only {
            "stylus"
        } else {
            "display"
        }
        .into();
    }
    cfg.pen_only = cfg.input_mode == "stylus";
    for p in cfg.tablets.values_mut() {
        if legacy_touchpad(&mut p.extra) && p.input_mode.is_none() {
            p.input_mode = Some("touchpad".into());
        }
    }
}

/// Serialise and write the config, creating the directory if needed.
pub async fn store(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    let toml_str =
        toml::to_string_pretty(config).map_err(|e| format!("Failed to serialize config: {e}"))?;
    tokio::fs::write(&path, toml_str)
        .await
        .map_err(|e| format!("Failed to write config: {e}"))
}

/// Tauri command: current config for the settings views.
#[tauri::command]
pub async fn get_config() -> Result<AppConfig, String> {
    Ok(load().await)
}

/// Settings that change the arguments of ffmpeg or evdi_helper.
fn stream_args_differ(a: &AppConfig, b: &AppConfig) -> bool {
    (
        &a.encoder,
        a.fps,
        a.bitrate,
        a.quality,
        a.stream_scale,
        a.ten_bit,
        a.width,
        a.height,
        a.hevc_low_latency_tuning,
        a.software_cursor,
    ) != (
        &b.encoder,
        b.fps,
        b.bitrate,
        b.quality,
        b.stream_scale,
        b.ten_bit,
        b.width,
        b.height,
        b.hevc_low_latency_tuning,
        b.software_cursor,
    )
}

fn profile_args_differ(a: Option<&TabletProfile>, b: &TabletProfile) -> bool {
    let key = |p: &TabletProfile| {
        (
            p.encoder.clone(),
            p.fps,
            p.bitrate,
            p.quality,
            p.stream_scale,
            p.ten_bit,
            p.width,
            p.height,
            p.width_mm,
            p.height_mm,
        )
    };
    a.map(key) != Some(key(b))
}

/// Tauri command: persist global settings and, if any of them changes the pipeline arguments, restart the affected pipelines.
#[tauri::command]
pub async fn save_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    let previous = load().await;
    store(&config).await?;
    // Placebo-settings fix: the daemon only read config.toml on tablet connect, so a save has
    // to tell it to respawn the pipelines of every device using the global values.
    if stream_args_differ(&previous, &config) {
        crate::daemon::restart_pipeline(&app, None).await?;
    }
    Ok(())
}

/// Tauri command: persist one device's profile and restart only that device's pipeline if its stream arguments changed.
#[tauri::command]
pub async fn save_device_profile(
    app: tauri::AppHandle,
    serial: String,
    profile: TabletProfile,
) -> Result<(), String> {
    let mut cfg = load().await;
    let changed = profile_args_differ(cfg.tablets.get(&serial), &profile);
    cfg.tablets.insert(serial.clone(), profile);
    store(&cfg).await?;
    if changed {
        crate::daemon::restart_pipeline(&app, Some(&serial)).await?;
    }
    Ok(())
}

/// Tauri command: store the user's saved fade curves right away (they are shared by every device page, which saves only its own profile).
#[tauri::command]
pub async fn save_curve_presets(presets: Vec<CurvePreset>) -> Result<(), String> {
    let mut cfg = load().await;
    cfg.curve_presets = presets;
    store(&cfg).await
}
