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
    /// Touchpad mode with native gestures: every finger that is down on the tablet's touch area
    /// right now, raw. The host replays them on a virtual multi-touch touchpad and libinput and
    /// the desktop recognise the gestures themselves. An empty list means all fingers lifted.
    #[serde(rename = "touchpad_raw")]
    TouchpadRaw { slots: Vec<RawSlot> },
    /// Touchpad gesture the tablet recognised that needs keys on the computer: `pinch` (Ctrl +
    /// scroll, `amount` in tablet pixels, positive = fingers apart) or a workspace switch
    /// (`workspace_prev` / `workspace_next`, Super + Page Up / Page Down).
    #[serde(rename = "gesture")]
    Gesture {
        kind: String,
        #[serde(default)]
        amount: f64,
    },
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

/// One finger of a [`InputEvent::TouchpadRaw`] frame.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RawSlot {
    /// The tablet's pointer id, stable for the life of the finger.
    pub id: i32,
    /// Position in millimetres from the top-left corner of the touch area.
    pub x: f64,
    pub y: f64,
    /// Android's 0..1 pressure estimate.
    #[serde(default)]
    pub pressure: f64,
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

/// What a numbered input message is, as far as the sequence filter cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqKind {
    /// Pen/touch move or hover, relative motion, raw touchpad frames: idempotent, never dropped.
    Move,
    /// A press (pen/touch ACTION_DOWN).
    Down,
    /// A lift (pen/touch ACTION_UP, a cancel reaches the host as one).
    Up,
    /// Anything else that is numbered (buttons, gestures, hover exit...).
    Other,
}

/// Drops input messages that arrive twice, or a press/lift that arrives after the opposite one that
/// was sent later.
///
/// The tablet numbers every input message it sends with a `seq` that only goes up. The rules are
/// deliberately narrow, because dropping the wrong message loses a drag:
///
/// * A move or hover is **never** dropped (moving twice to the same place is harmless, and a move
///   that overtakes its DOWN must not make the drag vanish).
/// * Any other message is dropped if its exact `seq` was already accepted (a duplicate).
/// * A DOWN is dropped only if a later-numbered UP was already accepted (it is the stale half of a
///   stroke that already ended; pressing now would leave the button stuck down). Otherwise a DOWN
///   that arrives after later moves is still accepted, so the drag goes on.
/// * An UP is dropped only if a later-numbered DOWN was already accepted (it would cut the new
///   stroke short).
///
/// Messages without a `seq`, and messages that are not input (the `rendered` frame acks carry a
/// frame counter in their own `seq` field), always pass.
#[derive(Debug, Default)]
pub struct SeqFilter {
    last_down: Option<u64>,
    last_up: Option<u64>,
    recent: std::collections::VecDeque<u64>,
    /// How many messages were discarded on this connection.
    pub dropped: u64,
}

impl SeqFilter {
    pub fn accept(&mut self, msg: Option<(u64, SeqKind)>) -> bool {
        let Some((seq, kind)) = msg else { return true };
        if kind == SeqKind::Move {
            return true;
        }
        let stale = self.recent.contains(&seq)
            || match kind {
                SeqKind::Down => self.last_up.is_some_and(|u| seq < u),
                SeqKind::Up => self.last_down.is_some_and(|d| seq < d),
                _ => false,
            };
        if stale {
            self.dropped += 1;
            return false;
        }
        match kind {
            SeqKind::Down => self.last_down = self.last_down.max(Some(seq)),
            SeqKind::Up => self.last_up = self.last_up.max(Some(seq)),
            _ => {}
        }
        if self.recent.len() >= 64 {
            self.recent.pop_front();
        }
        self.recent.push_back(seq);
        true
    }
}

/// The input `seq` and kind of a raw message, if it is a numbered input message (cheap check
/// before parsing). `rendered` and every non-input type give `None`.
pub fn classify(text: &str) -> Option<(u64, SeqKind)> {
    if !text.contains("\"seq\"") {
        return None;
    }
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "type")]
        kind: Option<String>,
        seq: Option<u64>,
        action: Option<u8>,
    }
    let e = serde_json::from_str::<Envelope>(text).ok()?;
    let seq = e.seq?;
    let kind = match (e.kind.as_deref()?, e.action) {
        ("pen", Some(0)) | ("touch", Some(0)) => SeqKind::Down,
        ("pen", Some(1)) | ("touch", Some(1)) => SeqKind::Up,
        ("pen", Some(2 | 3)) | ("touch", Some(2)) => SeqKind::Move,
        ("rel" | "scroll" | "touchpad_raw", _) => SeqKind::Move,
        ("pen" | "touch" | "button" | "gesture", _) => SeqKind::Other,
        _ => return None,
    };
    Some((seq, kind))
}

#[cfg(test)]
mod seq_tests {
    use super::*;
    use SeqKind::*;

    #[test]
    fn increasing_numbers_pass() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((1, Down))) && f.accept(Some((2, Move))) && f.accept(Some((10, Up))));
        assert_eq!(f.dropped, 0);
    }

    #[test]
    fn a_duplicate_down_is_dropped() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((5, Down))));
        assert!(!f.accept(Some((5, Down))));
        assert_eq!(f.dropped, 1);
    }

    #[test]
    fn moves_are_never_dropped_even_repeated_or_late() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((9, Move))));
        assert!(f.accept(Some((9, Move))));
        assert!(f.accept(Some((3, Move))));
        assert_eq!(f.dropped, 0);
    }

    #[test]
    fn a_down_that_arrives_after_later_moves_still_starts_the_drag() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((6, Move)))); // sent after the DOWN, arrived first
        assert!(f.accept(Some((7, Move))));
        assert!(f.accept(Some((5, Down))), "the DOWN must not be lost");
        assert!(f.accept(Some((8, Up))));
    }

    #[test]
    fn a_stale_down_after_its_up_is_dropped() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((7, Up)))); // UP (sent second) arrives first
        assert!(!f.accept(Some((6, Down)))); // its DOWN arrives late: pressing now would stick
        assert!(f.accept(Some((8, Down))));
    }

    #[test]
    fn a_stale_up_after_a_newer_down_is_dropped() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((4, Down))));
        assert!(!f.accept(Some((3, Up))));
    }

    #[test]
    fn messages_without_a_number_always_pass() {
        let mut f = SeqFilter::default();
        assert!(f.accept(Some((3, Down))));
        assert!(f.accept(None) && f.accept(None));
    }

    #[test]
    fn classify_reads_type_and_action() {
        assert_eq!(classify(r#"{"type":"pen","seq":42,"action":0}"#), Some((42, Down)));
        assert_eq!(classify(r#"{"type":"pen","seq":43,"action":2}"#), Some((43, Move)));
        assert_eq!(classify(r#"{"type":"pen","seq":44,"action":1}"#), Some((44, Up)));
        assert_eq!(classify(r#"{"type":"pen","seq":45,"action":5}"#), Some((45, Other)));
        assert_eq!(classify(r#"{"type":"rel","seq":46,"dx":1.0,"dy":0.0}"#), Some((46, Move)));
        assert_eq!(classify(r#"{"type":"pen","action":0}"#), None);
        assert_eq!(classify(r#"{"type":"pen","note":"seq","action":0}"#), None);
    }

    #[test]
    fn frame_acks_are_not_input_and_never_move_the_filter() {
        // `rendered` carries the video frame counter in `seq`; it used to be taken for an input
        // number, so once the counter was ahead every real pen message was discarded.
        assert_eq!(classify(r#"{"type":"rendered","seq":987654,"decode_us":300}"#), None);
        let mut f = SeqFilter::default();
        assert!(f.accept(classify(r#"{"type":"rendered","seq":987654}"#)));
        assert!(f.accept(classify(r#"{"type":"pen","seq":3,"action":0}"#)));
        assert_eq!(f.dropped, 0);
    }
}
