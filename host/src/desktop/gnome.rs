//! GNOME Shell on Wayland. A Wayland client cannot ask the compositor which window has focus, so
//! the `uscreen-app-lock` Shell extension (see `extensions/`) reports it in `focus.json`; this
//! backend reads that file. Verified on GNOME 46 (Ubuntu 24.04) only.

use super::focus_file::FocusFile;
use super::{Capabilities, DesktopBackend, FocusUpdate};

/// The GNOME backend: focus comes from the extension's file.
pub(super) struct GnomeDesktop {
    focus: FocusFile,
}

impl GnomeDesktop {
    pub(super) fn new() -> Self {
        Self { focus: FocusFile::default() }
    }
}

impl DesktopBackend for GnomeDesktop {
    fn name(&self) -> &'static str {
        "gnome"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { focus: true }
    }

    fn poll_focus(&mut self) -> FocusUpdate {
        self.focus.poll()
    }
}
