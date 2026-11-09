//! `TabletProfile`: the per-device section of config.toml and its helpers

use super::*;

/// What one device reported about itself, and anything set specifically for
/// it, keyed in the config by its adb serial.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct TabletProfile {
    /// Inferred model name (e.g. from ro.product.model).
    pub label: String,
    /// User-customized nickname for the device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Panel size in millimetres; feeds the EDID and so the desktop's scale.
    pub width_mm: u32,
    pub height_mm: u32,
    /// Set only where this device should differ from the global settings.
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
    /// Wi-Fi connection endpoint (IP:port) for wireless streaming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_address: Option<String>,
    /// Keep virtual display active when tablet disconnects (throttled to 1 FPS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<bool>,
    /// Application window locking: allowed apps on this monitor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_apps: Option<Vec<String>>,
    /// Input mode of this device (`display`, `touchpad`, `stylus`); unset inherits the global one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_mode: Option<InputMode>,
    /// Legacy override, read once to migrate to `input_mode`; never written.
    #[serde(skip_serializing)]
    pub touchpad_mode: Option<bool>,
    /// Hide the settings handle and draw under the cutout on the tablet. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub immersive_mode: Option<bool>,
    /// Show the fps / Mbps overlay on the tablet. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_stats: Option<bool>,
    /// Send finger touches to the computer (the pen always works). Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_input: Option<bool>,
    /// Forward touches while the tablet is locked. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_on_lock_screen: Option<bool>,
    /// Keep the app on screen over the tablet's lock screen. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_display_alive_when_locked: Option<bool>,
    /// Keep the sockets open when the app is in the background. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_connected_in_background: Option<bool>,
    /// Black out the tablet screen a few seconds after Stylus mode starts. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blank_pen_only: Option<bool>,
    /// Landscape direction on the tablet: auto, camera_up or camera_down. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
    /// Tablet app theme: system, light or dark. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_mode: Option<String>,
    /// After the host has been unreachable for two minutes: stop retrying and dim to 10%. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_saver_on_disconnect: Option<bool>,
    /// Battery saver also locks the tablet (needs the Device Admin permission). Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_saver_lock: Option<bool>,
    /// Dim the tablet while the pointer is away from it. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_dim_enabled: Option<bool>,
    /// Seconds of idleness before the tablet dims. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_dim_timeout_secs: Option<u32>,
    /// Dim fade curve: a preset name, `bezier:x1,y1,x2,y2` or `preset:<name>`. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim_curve: Option<String>,
    /// Undim fade curve (same forms as `dim_curve`). Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undim_curve: Option<String>,
    /// Length of the dim fade in milliseconds. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim_duration_ms: Option<u32>,
    /// Length of the undim fade in milliseconds. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undim_duration_ms: Option<u32>,
    /// Light vibration on touchpad clicks. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchpad_haptics: Option<bool>,
    /// Touchpad mode replays the tablet's fingers on a virtual multi-touch touchpad so the desktop
    /// recognises swipes and pinches itself. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_gestures: Option<bool>,
    /// Touchpad pointer and swipe sensitivity in percent (50-200). Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchpad_sensitivity: Option<u32>,
    /// How a touchpad right click is made: `bottom_right_zone` or `two_finger_tap`. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_click_method: Option<String>,
    /// Which part of the tablet acts as the touchpad: `full_screen`, `standard_rectangle` or `wide_rectangle`. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchpad_shape: Option<String>,
    /// What the pen does in Stylus mode: `mouse` (a pointer that clicks and drag-selects) or `tablet` (a pressure and tilt pen). Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylus_style: Option<String>,
    /// What the pen's first side button does in `mouse` style: `right`, `middle` or `none`. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylus_button_primary: Option<String>,
    /// What the pen's second side button does in `mouse` style: `right`, `middle` or `none`. Unset inherits the global value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stylus_button_secondary: Option<String>,
    /// Firmware/APK version reported by the connected Android client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// Dynamically discovered secure TLS connect port for Android 11+
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure_port: Option<u16>,
    /// Preferred transport ("auto", "usb", "wifi") requested by the user or APK
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<String>,
    /// `Some(false)` when wireless setup was skipped in the GUI wizard; `None` counts as configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wireless_configured: Option<bool>,
    /// Android API level (SDK int, e.g. 36)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_level: Option<u32>,
}

impl TabletProfile {
    /// Friendly display name: nickname if provided, else model label, else fallback.
    pub fn display_name<'a>(&'a self, fallback: &'a str) -> &'a str {
        if let Some(ref n) = self.nickname {
            let trimmed = n.trim();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
        if !self.label.trim().is_empty() {
            return self.label.trim();
        }
        fallback
    }

    /// Whether this device's own dimensions are known.
    pub fn has_display(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}
