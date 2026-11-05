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
pub const MIN_BITRATE_KBPS: u32 = 1_000;

/// The generated EDID caps the virtual mode at 90 Hz (EDID 1.4 stores the pixel
/// clock in 16 bits, and 2960x1848@120 overflows it), so anything above 90
/// would only ever produce duplicate frames.
pub const MAX_FPS: u32 = 90;
pub const MIN_FPS: u32 = 10;

/// Constant-quality target for the encoder (lower = sharper, more bits).
///
/// 18 rather than a more conservative value because bandwidth stopped being the
/// constraint: in constant-quality mode a desktop streams at a few Mbps against
/// a ceiling tens of times higher, so spending bits on crisp text is close to
/// free. Text sharpness is ultimately limited by 4:2:0 chroma subsampling, not
/// by this number — below roughly 16 there is nothing left to gain.
pub const DEFAULT_QUALITY: u32 = 18;
pub const MIN_QUALITY: u32 = 12;
pub const MAX_QUALITY: u32 = 32;

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
    /// Firmware/APK version reported by the connected Android client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// Dynamically discovered secure TLS connect port for Android 11+
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure_port: Option<u16>,
    /// Preferred transport ("auto", "usb", "wifi") requested by the user or APK
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<String>,
    /// Android API level (SDK int, e.g. 36)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_level: Option<u32>,
}

#[allow(dead_code)]
pub type DeviceProfile = TabletProfile;

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
    /// Protocol/daemon version.
    #[serde(default = "default_version")]
    pub version: String,
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
            locked_apps: Vec::new(),
            allow_legacy_fallback: false,
            auto_enable_wifi_adb: true,
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

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/uscreen/config.toml")
}

impl FileConfig {
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

        if Position::parse(&self.position).is_none() {
            tracing::warn!(
                "Unknown position {:?} — falling back to \"right\"",
                self.position
            );
            self.position = "right".into();
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).context("serialize config")?;
        // Say exactly which lines change. Settings that drift with nobody
        // touching them are impossible to chase down otherwise.
        if let Ok(old) = std::fs::read_to_string(&path) {
            let before: std::collections::BTreeMap<&str, &str> = old
                .lines()
                .filter_map(|l| l.split_once(" = "))
                .collect();
            let changed: Vec<String> = text
                .lines()
                .filter_map(|l| l.split_once(" = "))
                .filter(|(k, v)| before.get(k) != Some(v))
                .map(|(k, v)| format!("{} = {} (was {})", k, v, before.get(k).unwrap_or(&"<unset>")))
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
        assert_eq!(re_parsed.tablets.get("DEVICE_A").unwrap().encoder.as_deref(), Some("hevc_vaapi"));
    }

    #[test]
    fn device_profile_overrides_and_nickname_serialization() {
        let toml_data = r#"
encoder = "h264_vaapi"
fps = 60
quality = 18
bitrate = 20000

[tablets.R52X708AEHK]
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
        let profile = cfg.tablets.get("R52X708AEHK").expect("profile exists");
        assert_eq!(profile.display_name("R52X708AEHK"), "My Desk Tablet");
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
        let p2 = reparsed.tablets.get("R52X708AEHK").unwrap();
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
        assert_eq!(p.locked_apps.as_ref().unwrap(), &vec!["com.google.android.youtube"]);
    }
}
