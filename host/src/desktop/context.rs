//! What the desktop can tell us about the spot the pen is pointing at: which kind of surface is
//! under the pointer (an app window, the shell's own UI, the bare desktop), the window's frame, and
//! how much text is highlighted / on the clipboard. Used by the pen menu to decide whether and what
//! to offer. Only lengths are ever reported, never the text itself.

use serde::Deserialize;

/// The kind of surface under the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    /// The activities overview.
    Overview,
    /// The shell's own UI: top panel, dock, popups.
    Shell,
    /// Bare desktop / wallpaper.
    #[default]
    Desktop,
    /// An application window.
    Window,
}

/// A window's frame in desktop pixels.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A snapshot taken when the pen menu is requested.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PointerContext {
    /// The extension's clock (microseconds since the Unix epoch) when it answered.
    #[serde(default)]
    pub t: i64,
    /// Pointer position in desktop pixels.
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub zone: Zone,
    /// The frame of the window under the pointer (only for [`Zone::Window`]).
    #[serde(default)]
    pub frame: Option<Rect>,
    /// Whether that window draws its own title bar (client-side decorations).
    #[serde(default)]
    pub csd: bool,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub wm_class: String,
    /// Characters currently highlighted (the primary selection).
    #[serde(default)]
    pub selection_len: usize,
    /// Characters on the clipboard.
    #[serde(default)]
    pub clipboard_len: usize,
}

impl PointerContext {
    /// Parse the JSON a backend produced; `None` if it is not a context.
    pub fn from_json(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// How far below the top of the window under the pointer it is, in pixels.
    pub fn depth_in_window(&self) -> Option<f64> {
        self.frame.map(|f| self.y - f.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_the_extension_writes() {
        let c = PointerContext::from_json(
            r#"{"t":5,"x":624.5,"y":1025,"selection_len":12,"clipboard_len":0,"zone":"window",
                "frame":{"x":0,"y":40,"w":1920,"h":1040},"csd":true,"app_id":"x.desktop","wm_class":"X"}"#,
        )
        .unwrap();
        assert_eq!(c.zone, Zone::Window);
        assert_eq!((c.selection_len, c.clipboard_len), (12, 0));
        assert_eq!(c.depth_in_window(), Some(985.0));
        assert!(c.csd);
    }

    #[test]
    fn missing_fields_default_and_garbage_is_rejected() {
        let c = PointerContext::from_json(r#"{"x":1,"y":2}"#).unwrap();
        assert_eq!((c.zone, c.selection_len, c.frame), (Zone::Desktop, 0, None));
        assert!(PointerContext::from_json("not json").is_none());
    }
}
