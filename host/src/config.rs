//! `~/.config/uscreen/config.toml`: global stream settings plus one `[tablets.<id>]` profile per
//! paired device. Shared with the GUI, so keys must stay backwards compatible.
//!
//! Loading clamps every numeric setting to a sane range because values arrive from the tablet
//! and the GUI and, once saved, would otherwise persist a bad number forever.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Highest bitrate the USB transport actually sustains. Beyond this the encoder
/// outruns the link, frames pile up in every queue along the way and latency
/// grows without bound — the stream does not get sharper, only later.
///
/// This is a hard ceiling rather than a hint because the value is persisted:
/// a bad number pushed once from the tablet stays in the config file and keeps
/// poisoning every subsequent run.
pub const MAX_BITRATE_KBPS: u32 = 60_000;
/// Lowest bitrate accepted; below this the picture is unusable and rate control misbehaves.
pub const MIN_BITRATE_KBPS: u32 = 1_000;

/// The generated EDID caps the virtual mode at 90 Hz (EDID 1.4 stores the pixel
/// clock in 16 bits, and 2960x1848@120 overflows it), so anything above 90
/// would only ever produce duplicate frames.
pub const MAX_FPS: u32 = 90;
/// Lowest frame rate accepted.
pub const MIN_FPS: u32 = 10;

/// Constant-quality target for the encoder (lower = sharper, more bits).
///
/// 18 rather than a more conservative value because bandwidth stopped being the
/// constraint: in constant-quality mode a desktop streams at a few Mbps against
/// a ceiling tens of times higher, so spending bits on crisp text is close to
/// free. Text sharpness is ultimately limited by 4:2:0 chroma subsampling, not
/// by this number — below roughly 16 there is nothing left to gain.
pub const DEFAULT_QUALITY: u32 = 18;
/// Sharpest constant-quality value accepted; lower numbers only burn bandwidth.
pub const MIN_QUALITY: u32 = 12;
/// Blurriest constant-quality value accepted.
pub const MAX_QUALITY: u32 = 32;

/// Fade curves the tablet app knows how to animate; anything else falls back to the default.
pub const CURVES: [&str; 5] = [
    "linear",
    "ease_in",
    "ease_out",
    "ease_in_out",
    "exponential",
];
/// Highest and lowest control-point y of a custom Bezier fade (x is limited to 0..1).
pub const BEZIER_Y_MIN: f64 = -2.0;
pub const BEZIER_Y_MAX: f64 = 3.0;

/// How the tablet is used. Exactly one is active per device.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    /// Extended display: video is streamed and touch is absolute.
    #[default]
    Display,
    /// Laptop touchpad: no video, finger motion moves the pointer.
    Touchpad,
    /// Graphics tablet: no video, the pen drives the computer's own screen.
    Stylus,
}

impl InputMode {
    /// Whether this mode runs the capture and encoding pipeline.
    pub fn streams_video(self) -> bool {
        self == InputMode::Display
    }

    /// Wire / config name.
    pub fn as_str(self) -> &'static str {
        match self {
            InputMode::Display => "display",
            InputMode::Touchpad => "touchpad",
            InputMode::Stylus => "stylus",
        }
    }
}

/// A fade curve the user saved in the GUI: a CSS-style cubic Bezier through (0,0) and (1,1).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct CurvePreset {
    pub name: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Parse `bezier:x1,y1,x2,y2` into clamped control points.
fn parse_bezier(text: &str) -> Option<(f64, f64, f64, f64)> {
    let rest = text.strip_prefix("bezier:")?;
    let v: Vec<f64> = rest
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if v.len() != 4 || v.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let x = |n: f64| n.clamp(0.0, 1.0);
    let y = |n: f64| n.clamp(BEZIER_Y_MIN, BEZIER_Y_MAX);
    Some((x(v[0]), y(v[1]), x(v[2]), y(v[3])))
}

/// The wire form the tablet understands for a stored curve: a preset name, or
/// `bezier:x1,y1,x2,y2` for a custom or saved one (`preset:<name>` is looked up in `presets`).
/// Anything unusable becomes `fallback`.
pub fn resolve_curve(curve: &str, presets: &[CurvePreset], fallback: &str) -> String {
    let c = curve.trim();
    if let Some(name) = c.strip_prefix("preset:") {
        return presets
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| parse_bezier(&format!("bezier:{},{},{},{}", p.x1, p.y1, p.x2, p.y2)))
            .map(|b| format!("bezier:{},{},{},{}", b.0, b.1, b.2, b.3))
            .unwrap_or_else(|| fallback.to_string());
    }
    if let Some(b) = parse_bezier(c) {
        return format!("bezier:{},{},{},{}", b.0, b.1, b.2, b.3);
    }
    let lower = c.to_ascii_lowercase();
    if CURVES.contains(&lower.as_str()) {
        lower
    } else {
        fallback.to_string()
    }
}

/// Shortest and longest idle timeout (seconds) before the tablet is dimmed.
pub const MIN_IDLE_TIMEOUT_SECS: u32 = 10;
pub const MAX_IDLE_TIMEOUT_SECS: u32 = 300;
/// Shortest and longest fade (milliseconds) of the idle dim / undim animation.
pub const MIN_FADE_MS: u32 = 100;
pub const MAX_FADE_MS: u32 = 30_000;

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

/// Persistent settings, shared by the CLI daemon, the GUI and the tablet app
/// (which pushes changes over the input WebSocket).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct FileConfig {
    pub encoder: String,
    pub fps: u32,
    /// kbps
    pub bitrate: u32,
    pub width: u32,
    pub height: u32,
    /// Encoder constant-quality target: lower is sharper and costs more bits.
    ///
    /// This, not the bitrate, is what governs picture quality now that the
    /// encoder runs in constant-quality mode — the bitrate is only a ceiling
    /// for bursts, and on a desktop the stream sits far below it.
    pub quality: u32,
    /// Integer downscale for the stream only; the desktop keeps its native
    /// mode. 1 = native, 2 = half. Trades sharpness for latency: the tablet's
    /// decoder costs roughly 7-8ms fixed plus 1.2ms per megapixel, so fewer
    /// pixels arrive on screen sooner.
    pub stream_scale: u32,
    /// Use the tablet as a graphics tablet for the laptop's own screen rather
    /// than as a second display: no capture, no encoding, nothing streamed —
    /// the pen and touch simply drive the screen you are already looking at.
    /// For drawing that removes display latency from the loop entirely, which
    /// is worth more than any amount of tuning the video path.
    pub pen_only: bool,
    /// Where the virtual screen sits relative to the physical ones:
    /// "right" (default), "left", "above" or "below". Anything else is
    /// treated as "right" rather than refusing to start over a typo.
    pub position: String,
    /// Encode in 10 bits (HEVC Main10). The captured desktop is 8-bit and
    /// cannot be otherwise, so this adds no colour — it gives the encoder
    /// more precision to work in, which is what removes banding from
    /// gradients at a given bitrate. Costs a format conversion per frame.
    pub ten_bit: bool,
    /// Require the tablet to present this run's session token before it is
    /// sent any video or allowed to inject input. The token is handed to the
    /// app over adb when it is launched. Without this, any local process —
    /// or any other app on the tablet — could connect to the loopback ports,
    /// read the screen and drive the mouse. Off only if you need an app
    /// older than 1.1.0 to keep working.
    pub require_token: bool,
    /// Ask GitHub once a day whether a newer release exists, and say so in
    /// the tray and in `uscreen doctor`. Nothing is ever downloaded or
    /// installed by the daemon; updating stays with you or your package
    /// manager. One HTTPS request a day to api.github.com.
    pub check_updates: bool,
    /// How many tablets may be attached at once, each as its own virtual
    /// screen. Needs that many EVDI devices: the installer sets
    /// initial_device_count to match. 1 keeps everything exactly as before.
    pub max_tablets: u32,
    /// The tablet's address on the network, as `ip:port`, remembered by
    /// `uscreen wifi`. When the cable is not plugged in the daemon tries to
    /// reconnect to it by itself, so the tablet comes back as a screen
    /// without anyone typing an adb command. Empty disables that.
    pub wifi_address: String,
    /// The DRM render node VAAPI encodes on, e.g. `/dev/dri/renderD129`.
    /// Empty picks the first node that really encodes a frame: on a laptop
    /// with two GPUs `renderD128` is whichever driver loaded first, and when
    /// that is NVIDIA's (which cannot encode through VAAPI) a perfectly good
    /// Intel or AMD encoder next to it went unused.
    pub vaapi_device: String,
    /// Remembered per device, keyed by adb serial. Written when a device
    /// reports its panel, read the next time that device is plugged in.
    #[serde(alias = "devices")]
    pub tablets: std::collections::HashMap<String, TabletProfile>,
    /// Leave an ordinary mouse cursor where the pen was lifted. A tablet
    /// tool's cursor disappears when it leaves proximity, which loses the
    /// place you were pointing at; the "UScreen Pointer" device parks the
    /// desktop cursor there instead. The price is that the shared cursor now
    /// sits on the tablet's screen and has to be brought back with the mouse,
    /// and on mixed-scale layouts KWin places an absolute pointer slightly
    /// off (#18). `false` creates no such device: the pen and touch drive
    /// the tablet's screen, the mouse stays wherever it was.
    pub pointer_handoff: bool,
    /// Match the virtual display to whatever resolution the tablet reports
    pub auto_resolution: bool,
    pub video_port: u16,
    pub input_port: u16,
    /// Launch the UScreen app on the tablet automatically when it's plugged in
    pub auto_launch_app: bool,
    /// When enabled and system power profile is "performance", forces 60 FPS stream
    /// even when cursor is on the primary display.
    pub max_performance_mode: bool,
    /// When enabled, applies aggressive low-latency tuning flags to HEVC VAAPI encoder.
    pub hevc_low_latency_tuning: bool,
    /// Keep virtual display active when tablet disconnects (throttled to 1 FPS).
    pub keep_alive: bool,
    /// Compatibility mode for the mouse cursor on the virtual screen: plain
    /// libevdi compositing without the X11 boundary-nudge helper (slower, but it
    /// depends on nothing compositor-specific). Off = the fast path.
    #[serde(default)]
    pub software_cursor: bool,
    /// Application window locking: allowed apps on the virtual monitor.
    #[serde(default)]
    pub locked_apps: Vec<String>,
    /// Allow legacy plaintext tcpip fallback for devices running Android <11 (API <30).
    /// Defaults to false for security and port collision prevention.
    #[serde(default)]
    pub allow_legacy_fallback: bool,
    /// Automatically enable Wi-Fi ADB when a device is plugged in via USB.
    #[serde(default = "default_true")]
    pub auto_enable_wifi_adb: bool,
    /// Stream any device that shows up on adb, and save a profile for it. Off by default: a
    /// device is paired explicitly (GUI "Add device"), which creates its `[tablets.<serial>]`
    /// entry. Until then it is a "new device" and the daemon leaves it alone.
    #[serde(default)]
    pub auto_pair_new_devices: bool,
    /// Input mode default: `display` (extended screen), `touchpad` or `stylus`. Absent in old
    /// files; sanitising fills it from the legacy `pen_only` / `touchpad_mode` flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_mode: Option<InputMode>,
    /// Legacy flag, read once to migrate to `input_mode`; never written.
    #[serde(default, skip_serializing)]
    pub touchpad_mode: bool,
    /// Hide the settings handle and draw under the cutout on the tablet.
    pub immersive_mode: bool,
    /// Show the fps / Mbps overlay on the tablet.
    pub show_stats: bool,
    /// Send finger touches to the computer (the pen always works).
    pub touch_input: bool,
    /// Forward touches while the tablet is locked.
    pub touch_on_lock_screen: bool,
    /// Keep the app on screen over the tablet's lock screen.
    pub keep_display_alive_when_locked: bool,
    /// Keep the sockets open when the app is in the background.
    pub keep_connected_in_background: bool,
    /// Black out the tablet screen a few seconds after Stylus mode starts.
    pub blank_pen_only: bool,
    /// Landscape direction on the tablet: auto, camera_up or camera_down.
    pub orientation: String,
    /// Tablet app theme: system, light or dark.
    pub theme_mode: String,
    /// After the host has been unreachable for two minutes: stop retrying and dim to 10%.
    pub battery_saver_on_disconnect: bool,
    /// Battery saver also locks the tablet (needs the Device Admin permission).
    pub battery_saver_lock: bool,
    /// Dim the tablet while the pointer is away from it.
    pub idle_dim_enabled: bool,
    /// Seconds of idleness before the tablet dims.
    pub idle_dim_timeout_secs: u32,
    /// Dim fade curve: a preset name, `bezier:x1,y1,x2,y2` or `preset:<name>`.
    pub dim_curve: String,
    /// Undim fade curve (same forms as `dim_curve`).
    pub undim_curve: String,
    /// Length of the dim fade in milliseconds.
    pub dim_duration_ms: u32,
    /// Length of the undim fade in milliseconds.
    pub undim_duration_ms: u32,
    /// Light vibration on touchpad clicks.
    pub touchpad_haptics: bool,
    /// Use native gestures (Wayland / libinput) in Touchpad mode: the tablet's fingers are replayed
    /// on a virtual multi-touch touchpad and GNOME / KDE handle swipes and pinches themselves.
    /// Off, or on X11 and desktops without libinput gestures, the tablet recognises gestures and
    /// the host sends keys.
    pub native_gestures: bool,
    /// How a touchpad right click is made: `bottom_right_zone` or `two_finger_tap`.
    pub right_click_method: String,
    /// Which part of the tablet acts as the touchpad: `full_screen`, `standard_rectangle` or `wide_rectangle`.
    pub touchpad_shape: String,
    /// What the pen does in Stylus mode: `mouse` or `tablet`.
    pub stylus_style: String,
    /// First pen side button in `mouse` style: `right`, `middle` or `none`.
    pub stylus_button_primary: String,
    /// Second pen side button in `mouse` style: `right`, `middle` or `none`.
    pub stylus_button_secondary: String,
    /// Fade curves saved by the user in the GUI's curve editor, usable as `preset:<name>`.
    pub curve_presets: Vec<CurvePreset>,
    /// Protocol/daemon version.
    #[serde(default = "default_version")]
    pub version: String,
}

/// `full_screen` (default), `standard_rectangle` or `wide_rectangle`; anything else falls back to the default.
pub fn clean_shape(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        s @ ("standard_rectangle" | "wide_rectangle") => s.to_string(),
        _ => "full_screen".into(),
    }
}

/// `mouse` (default) or `tablet`.
pub fn clean_stylus_style(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "tablet" => "tablet".into(),
        _ => "mouse".into(),
    }
}

/// `right`, `middle` or `none`; anything else falls back to [default].
pub fn clean_pen_button(v: &str, default: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        b @ ("right" | "middle" | "none") => b.to_string(),
        _ => default.to_string(),
    }
}

/// `bottom_right_zone` (default) or `two_finger_tap`; anything else falls back to the default.
fn clean_right_click(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "two_finger_tap" => "two_finger_tap".into(),
        _ => "bottom_right_zone".into(),
    }
}

fn default_true() -> bool {
    true
}

fn default_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            encoder: "h264_nvenc".into(),
            fps: 60,
            bitrate: 20000,
            width: 2960,
            height: 1848,
            quality: DEFAULT_QUALITY,
            stream_scale: 1,
            pen_only: false,
            position: "right".into(),
            ten_bit: false,
            require_token: true,
            check_updates: true,
            max_tablets: 1,
            wifi_address: String::new(),
            vaapi_device: String::new(),
            tablets: std::collections::HashMap::new(),
            pointer_handoff: true,
            auto_resolution: true,
            video_port: 8890,
            input_port: 8891,
            auto_launch_app: true,
            max_performance_mode: false,
            hevc_low_latency_tuning: true,
            keep_alive: false,
            software_cursor: false,
            locked_apps: Vec::new(),
            allow_legacy_fallback: false,
            auto_enable_wifi_adb: true,
            auto_pair_new_devices: false,
            input_mode: Some(InputMode::Display),
            touchpad_mode: false,
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
            right_click_method: "bottom_right_zone".into(),
            touchpad_shape: "full_screen".into(),
            stylus_style: "mouse".into(),
            stylus_button_primary: "right".into(),
            stylus_button_secondary: "middle".into(),
            curve_presets: Vec::new(),
        }
    }
}

/// Where the virtual screen goes relative to everything already on the desktop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Position {
    Right,
    Left,
    Above,
    Below,
}

impl Position {
    /// Parse a position name case-insensitively; `top`/`bottom` are accepted as aliases of `above`/`below`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "right" => Some(Position::Right),
            "left" => Some(Position::Left),
            "above" | "top" => Some(Position::Above),
            "below" | "bottom" => Some(Position::Below),
            _ => None,
        }
    }

    /// Unknown values are a typo in a config file, not a reason to refuse to
    /// bring the screen up at all.
    pub fn parse_or_default(s: &str) -> Self {
        Self::parse(s).unwrap_or(Position::Right)
    }
}

/// Location of `config.toml`, shared with the Tauri GUI (`~/.config/uscreen/config.toml`).
pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/uscreen/config.toml")
}

impl FileConfig {
    /// Read and sanitise the config file.
    ///
    /// A missing or unparsable file yields defaults, and out-of-range values are
    /// clamped (with a warning) so a bad number pushed once cannot poison every
    /// later run.
    pub fn load() -> Self {
        let path = config_path();
        let mut cfg = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("Invalid config at {:?}: {} — using defaults", path, e);
                Self::default()
            }),
            Err(_) => Self::default(),
        };
        cfg.sanitize();
        cfg
    }

    /// Pull persisted values back into the range the pipeline can actually
    /// serve. Older builds let the tablet push 200 Mbps @ 90 fps and wrote it
    /// straight to disk, so existing installs carry settings that guarantee
    /// multi-second latency until they are clamped here.
    pub fn sanitize(&mut self) {
        let bitrate = self.bitrate.clamp(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS);
        if bitrate != self.bitrate {
            tracing::warn!(
                "Bitrate {} kbps is beyond what the USB transport sustains — clamped to {} kbps",
                self.bitrate,
                bitrate
            );
            self.bitrate = bitrate;
        }

        let fps = self.fps.clamp(MIN_FPS, MAX_FPS);
        if fps != self.fps {
            tracing::warn!("fps {} out of range — clamped to {}", self.fps, fps);
            self.fps = fps;
        }

        self.quality = self.quality.clamp(MIN_QUALITY, MAX_QUALITY);
        self.stream_scale = self.stream_scale.clamp(1, 4);
        self.max_tablets = self.max_tablets.clamp(1, 4);
        self.width = self.width.clamp(640, 8192);
        self.height = self.height.clamp(480, 8192);
        self.idle_dim_timeout_secs = self
            .idle_dim_timeout_secs
            .clamp(MIN_IDLE_TIMEOUT_SECS, MAX_IDLE_TIMEOUT_SECS);
        self.dim_duration_ms = self.dim_duration_ms.clamp(MIN_FADE_MS, MAX_FADE_MS);
        self.undim_duration_ms = self.undim_duration_ms.clamp(MIN_FADE_MS, MAX_FADE_MS);
        self.curve_presets.retain(|p| !p.name.trim().is_empty());
        for p in &mut self.curve_presets {
            p.name = p.name.trim().to_string();
            p.x1 = p.x1.clamp(0.0, 1.0);
            p.x2 = p.x2.clamp(0.0, 1.0);
            p.y1 = p.y1.clamp(BEZIER_Y_MIN, BEZIER_Y_MAX);
            p.y2 = p.y2.clamp(BEZIER_Y_MIN, BEZIER_Y_MAX);
        }
        for (curve, fallback) in [
            (&mut self.dim_curve, "ease_out"),
            (&mut self.undim_curve, "ease_in"),
        ] {
            // A `preset:` reference stays even when its preset was deleted: it resolves to the
            // default wherever it is used. Everything else is normalised here.
            if !curve.starts_with("preset:") {
                *curve = resolve_curve(curve, &self.curve_presets, fallback);
            }
        }
        self.orientation = match self.orientation.trim().to_ascii_lowercase().as_str() {
            o @ ("auto" | "camera_up" | "camera_down") => o.to_string(),
            _ => "auto".into(),
        };
        self.theme_mode = match self.theme_mode.trim().to_ascii_lowercase().as_str() {
            t @ ("system" | "light" | "dark") => t.to_string(),
            _ => "system".into(),
        };
        self.right_click_method = clean_right_click(&self.right_click_method);
        self.touchpad_shape = clean_shape(&self.touchpad_shape);
        self.stylus_style = clean_stylus_style(&self.stylus_style);
        self.stylus_button_primary = clean_pen_button(&self.stylus_button_primary, "right");
        self.stylus_button_secondary = clean_pen_button(&self.stylus_button_secondary, "middle");
        for p in self.tablets.values_mut() {
            if let Some(m) = p.stylus_style.take() {
                p.stylus_style = Some(clean_stylus_style(&m));
            }
            if let Some(m) = p.stylus_button_primary.take() {
                p.stylus_button_primary = Some(clean_pen_button(&m, "right"));
            }
            if let Some(m) = p.stylus_button_secondary.take() {
                p.stylus_button_secondary = Some(clean_pen_button(&m, "middle"));
            }
            if let Some(m) = p.right_click_method.take() {
                p.right_click_method = Some(clean_right_click(&m));
            }
            if let Some(m) = p.touchpad_shape.take() {
                p.touchpad_shape = Some(clean_shape(&m));
            }
        }
        // One-time migration from the two booleans this enum replaced.
        if self.input_mode.is_none() {
            self.input_mode = Some(if self.touchpad_mode {
                InputMode::Touchpad
            } else if self.pen_only {
                InputMode::Stylus
            } else {
                InputMode::Display
            });
        }
        self.touchpad_mode = false;
        self.pen_only = self.input_mode == Some(InputMode::Stylus);
        for p in self.tablets.values_mut() {
            if p.input_mode.is_none() && p.touchpad_mode == Some(true) {
                p.input_mode = Some(InputMode::Touchpad);
            }
            p.touchpad_mode = None;
        }

        if Position::parse(&self.position).is_none() {
            tracing::warn!(
                "Unknown position {:?} — falling back to \"right\"",
                self.position
            );
            self.position = "right".into();
        }
    }

    /// Make Stylus (`true`) or Display (`false`) the global default input mode. Returns whether that
    /// changed anything. A global Touchpad default is left alone when the request is for Display:
    /// the tray's switch is a Stylus toggle, it must not undo a Touchpad default chosen separately.
    pub fn set_global_stylus(&mut self, on: bool) -> bool {
        let current = self.input_mode.unwrap_or_default();
        let next = match (on, current) {
            (true, InputMode::Stylus) => return false,
            (true, _) => InputMode::Stylus,
            (false, InputMode::Stylus) => InputMode::Display,
            (false, _) => return false,
        };
        self.input_mode = Some(next);
        true
    }

    /// Whether a device has a saved profile, under its hardware serial or the adb serial it is
    /// currently reached through (older profiles are keyed by `ip:port`).
    pub fn is_paired(&self, hw_id: &str, serial: &str) -> bool {
        self.auto_pair_new_devices
            || self.tablets.contains_key(hw_id)
            || self.tablets.contains_key(serial)
    }

    /// Write the config back as pretty TOML, logging which lines changed so settings that drift unexpectedly can be traced.
    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).context("serialize config")?;
        // Say exactly which lines change. Settings that drift with nobody
        // touching them are impossible to chase down otherwise.
        if let Ok(old) = std::fs::read_to_string(&path) {
            let before: std::collections::BTreeMap<&str, &str> =
                old.lines().filter_map(|l| l.split_once(" = ")).collect();
            let changed: Vec<String> = text
                .lines()
                .filter_map(|l| l.split_once(" = "))
                .filter(|(k, v)| before.get(k) != Some(v))
                .map(|(k, v)| {
                    format!(
                        "{} = {} (was {})",
                        k,
                        v,
                        before.get(k).unwrap_or(&"<unset>")
                    )
                })
                .collect();
            if !changed.is_empty() {
                tracing::info!("Config written: {}", changed.join(", "));
            }
        }
        // Write-then-rename: a reader must never see a half-written file.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).context("write config file")?;
        std::fs::rename(&tmp, &path).context("replace config file")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tray_toggle_changes_only_the_stylus_default() {
        let mut c = FileConfig::default();
        assert!(c.set_global_stylus(true));
        assert_eq!(c.input_mode, Some(InputMode::Stylus));
        assert!(!c.set_global_stylus(true), "already Stylus: nothing to write");
        assert!(c.set_global_stylus(false));
        assert_eq!(c.input_mode, Some(InputMode::Display));
        // A Touchpad default chosen separately survives the switch being turned off.
        c.input_mode = Some(InputMode::Touchpad);
        assert!(!c.set_global_stylus(false));
        assert_eq!(c.input_mode, Some(InputMode::Touchpad));
    }

    #[test]
    fn stylus_settings_are_validated() {
        assert_eq!(clean_pen_button(" Right ", "middle"), "right");
        assert_eq!(clean_pen_button("bogus", "middle"), "middle");
        assert_eq!(clean_stylus_style("TABLET"), "tablet");
        assert_eq!(clean_stylus_style("x"), "mouse");
        let mut c = FileConfig {
            stylus_button_primary: "nonsense".into(),
            ..FileConfig::default()
        };
        c.sanitize();
        assert_eq!(c.stylus_button_primary, "right");
    }

    use super::*;

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
}
