//! The input mode (display / touchpad / stylus) and the tablet's position next to the screen

use super::*;

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
