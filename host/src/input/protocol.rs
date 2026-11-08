//! Wire types of the input socket: the JSON the tablet sends ([`InputEvent`]), the handshake reply
//! the host sends back ([`InputResponse`]) and the per-server configuration ([`InputConfig`]).

use super::*;

/// One JSON message from the tablet's input WebSocket (touch, pen, mouse, keys, settings, pings...), discriminated by its `type` field.
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum InputEvent {
    #[serde(rename = "touch")]
    Touch {
        x: f64,
        y: f64,
        pressure: f64,
        action: u8,
        slot: u8,
    },
    #[serde(rename = "pen")]
    Pen {
        x: f64,
        y: f64,
        pressure: f64,
        tilt_x: f64,
        tilt_y: f64,
        /// True when the pen's eraser end is in use (TOOL_TYPE_ERASER).
        /// Emitted as BTN_TOOL_RUBBER so GIMP's eraser works.
        #[serde(default)]
        eraser: bool,
        /// 0=down 1=up 2=move 3=hover 4=hover_exit
        /// 5=stylus button down 6=stylus button up
        action: u8,
    },
    #[serde(rename = "resolution")]
    Resolution {
        width: u32,
        height: u32,
        /// Physical panel size, when the tablet knows it. Feeds the EDID so
        /// the desktop derives the right DPI and default scale.
        #[serde(default)]
        width_mm: u32,
        #[serde(default)]
        height_mm: u32,
        #[serde(default)]
        preferred_transport: Option<String>,
        #[serde(default)]
        version: Option<String>,
    },
    /// Settings pushed from the tablet app's settings UI
    #[serde(rename = "config")]
    Config {
        bitrate: Option<u32>,
        fps: Option<u32>,
        encoder: Option<String>,
    },
    /// The tablet asking to switch between being a second screen and being a
    /// graphics tablet. Applied live: the host remaps the input devices onto
    /// the other output and brings the virtual display up or down to match.
    #[serde(rename = "mode")]
    Mode { pen_only: bool },
    /// Touchpad mode: the finger moved the pointer by this many tablet pixels, relative to where
    /// it was. The tablet does the gesture recognition; the computer only gets plain mouse
    /// events.
    #[serde(rename = "rel")]
    Rel { dx: f64, dy: f64 },
    /// Touchpad mode: a mouse button went down or up. 0 = left, 1 = right, 2 = middle.
    #[serde(rename = "button")]
    Button { button: u8, down: bool },
    /// Touchpad mode: two-finger scroll, in tablet pixels. Positive `dy` is the wheel turning
    /// forward (the content moves down), positive `dx` the content moving right; the tablet
    /// picks the direction it wants.
    #[serde(rename = "scroll")]
    Scroll { dx: f64, dy: f64 },
    /// Touchpad gesture the tablet recognised that needs a modifier held on the computer:
    /// `pinch` (Ctrl + scroll, `amount` in tablet pixels, positive = fingers apart), `rotate`
    /// (Alt + scroll, positive = clockwise) or a workspace switch (`workspace_prev` /
    /// `workspace_next`, Super + Page Up / Page Down).
    #[serde(rename = "gesture")]
    Gesture {
        kind: String,
        #[serde(default)]
        amount: f64,
    },
    /// Two-finger pinch / rotate on the tablet, as the change since the last message: `zoom` is the
    /// factor the finger spread changed by (above 1 is apart) and `rotation` the turn in degrees
    /// (positive is clockwise). The host replays it as two fingers on a virtual touchpad, so the
    /// compositor and the application see a native pinch gesture.
    #[serde(rename = "transform")]
    Transform { zoom: f64, rotation: f64 },
    /// The two-finger transform is over: lift the virtual fingers.
    #[serde(rename = "transform_end")]
    TransformEnd,
    /// A stylus fact for the active stylus profile: `button_secondary` / `air_button`
    /// (`pressed`), `air_motion` (`dx`, `dy`) or `orientation` (`radians`). The primary side
    /// button keeps arriving as a pen action 5 / 6.
    #[serde(rename = "stylus_event")]
    StylusEvent {
        event: String,
        #[serde(default)]
        pressed: bool,
        #[serde(default)]
        dx: f64,
        #[serde(default)]
        dy: f64,
        #[serde(default)]
        radians: f64,
    },
    /// Run one stylus action now, so a profile mapping can be tried before it is saved.
    #[serde(rename = "stylus_test")]
    StylusTest { action: String },
    /// Create or replace a custom stylus profile (the JSON file's contents).
    #[serde(rename = "stylus_profile_save")]
    StylusProfileSave { profile: serde_json::Value },
    /// Delete a custom stylus profile.
    #[serde(rename = "stylus_profile_delete")]
    StylusProfileDelete { id: String },
    /// The user touched the tablet while it was dimmed, or put a finger on the touchpad.
    #[serde(rename = "wake")]
    Wake,
    /// A setting toggled on the tablet itself (`touchpad_mode`, `immersive_mode`,
    /// `battery_saver_on_disconnect`); stored as this device's override and echoed back to the
    /// app as a `config_update` once applied.
    #[serde(rename = "setting")]
    Setting {
        key: String,
        value: serde_json::Value,
    },
    /// Must be the first message on the socket. Proves the client is the
    /// tablet this daemon launched, not some other process on the loopback.
    #[serde(rename = "auth")]
    Auth { token: String },
    /// The tablet has this frame on screen. Closes the latency measurement
    /// loop — the host timed the frame out, so the round trip needs no clock
    /// agreement between the two devices.
    #[serde(rename = "rendered")]
    Rendered {
        seq: u32,
        /// Microseconds the tablet spent between receiving the frame and
        /// putting it on screen. Subtracting it from the round trip isolates
        /// what the transport actually costs.
        #[serde(default)]
        decode_us: i64,
    },
    /// Foreground hello/reconnect ping from the device app onStart/onResume.
    #[serde(rename = "ping")]
    Ping,
    /// Explicit preferred transport requested by the APK ("auto", "usb", "wifi")
    #[serde(rename = "preferred_transport")]
    PreferredTransport { transport: String },
}

/// The host's handshake reply after authentication: display size, codec and the current stream settings.
#[derive(Serialize)]
pub struct InputResponse {
    pub status: String,
    pub width: u32,
    pub height: u32,
    /// Which bitstream the tablet should expect: "h264" or "hevc". It has to
    /// build the decoder before the first frame arrives, and the frames
    /// themselves carry nothing that identifies the codec.
    pub codec: String,
    /// Tells the tablet not to expect a video stream: it is acting as a
    /// graphics tablet for the host's own screen, not as a display.
    pub pen_only: bool,
    /// The computer's current stream settings. The tablet shows these and
    /// sends settings back only when the user presses Apply; until it knew
    /// them it re-sent its own remembered copy on every connect, which
    /// overwrote whatever was set in `config.toml` or the GUI (#37). Absent
    /// from a daemon that does not know about them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    /// Git version string of the host binary for client-side version verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_version: Option<String>,
}

/// Static configuration of one tablet's input server.
#[derive(Clone)]
pub struct InputConfig {
    pub port: u16,
    /// Which tablet this server belongs to (0 = the first). Decides device
    /// names, product ids, and which virtual output the devices map to.
    pub instance: u32,
    /// Session token the client must present first; `None` disables it.
    pub token: Option<String>,
    /// Bitstream the encoder produces, so connecting clients can be told.
    pub codec: String,
    pub virtual_width: u32,
    pub virtual_height: u32,
    /// Whether to create the "UScreen Pointer" device at all; see
    /// `FileConfig::pointer_handoff`.
    pub pointer_handoff: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            port: 8891,
            instance: 0,
            token: None,
            codec: "h264".into(),
            virtual_width: 2960,
            virtual_height: 1848,
            pointer_handoff: true,
        }
    }
}
