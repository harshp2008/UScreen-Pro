//! Persistent configuration (`~/.config/uscreen/config.toml`) shared with the host daemon.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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
    pub keep_alive: bool,
    #[serde(default)]
    pub locked_apps: Vec<String>,
    pub allow_legacy_fallback: bool,
    pub auto_enable_wifi_adb: bool,
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
            keep_alive: false,
            locked_apps: Vec::new(),
            allow_legacy_fallback: false,
            auto_enable_wifi_adb: true,
            version: env!("GIT_VERSION_STRING").to_string(),
            tablets: HashMap::new(),
            extra: toml::value::Table::new(),
        }
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn config_path() -> PathBuf {
    home_dir().join(".config/uscreen/config.toml")
}

pub async fn load() -> AppConfig {
    match tokio::fs::read_to_string(config_path()).await {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub async fn store(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    tokio::fs::write(&path, toml_str)
        .await
        .map_err(|e| format!("Failed to write config: {e}"))
}

#[tauri::command]
pub async fn get_config() -> Result<AppConfig, String> {
    Ok(load().await)
}

/// Settings that change the arguments of ffmpeg or evdi_helper.
fn stream_args_differ(a: &AppConfig, b: &AppConfig) -> bool {
    (&a.encoder, a.fps, a.bitrate, a.quality, a.stream_scale, a.ten_bit, a.width, a.height)
        != (&b.encoder, b.fps, b.bitrate, b.quality, b.stream_scale, b.ten_bit, b.width, b.height)
}

fn profile_args_differ(a: Option<&TabletProfile>, b: &TabletProfile) -> bool {
    let key = |p: &TabletProfile| {
        (p.encoder.clone(), p.fps, p.bitrate, p.quality, p.stream_scale, p.ten_bit, p.width, p.height, p.width_mm, p.height_mm)
    };
    a.map(key) != Some(key(b))
}

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

#[tauri::command]
pub async fn save_device_profile(app: tauri::AppHandle, serial: String, profile: TabletProfile) -> Result<(), String> {
    let mut cfg = load().await;
    let changed = profile_args_differ(cfg.tablets.get(&serial), &profile);
    cfg.tablets.insert(serial.clone(), profile);
    store(&cfg).await?;
    if changed {
        crate::daemon::restart_pipeline(&app, Some(&serial)).await?;
    }
    Ok(())
}
