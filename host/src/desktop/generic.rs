//! The fallback backend for a desktop we have no integration for (KDE Wayland, Sway, Hyprland, ...).
//! It still reads `focus.json`, so a user script can feed it, but it claims no capabilities: the
//! pen stays on the mouse route unless the route is flipped by hand (`uscreen toggle-tablet`).

use super::focus_file::FocusFile;
use super::{Capabilities, DesktopBackend, FocusUpdate};

/// The generic backend.
pub(super) struct GenericDesktop {
    focus: FocusFile,
}

impl GenericDesktop {
    /// A backend that has not seen a focus change yet.
    pub(super) fn new() -> Self {
        Self {
            focus: FocusFile::default(),
        }
    }
}

impl DesktopBackend for GenericDesktop {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn poll_focus(&mut self) -> FocusUpdate {
        self.focus.poll()
    }
}
