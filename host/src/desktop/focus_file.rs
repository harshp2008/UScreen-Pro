//! Reads `~/.local/share/uscreen/focus.json`, the file a helper (the GNOME Shell extension) writes
//! on every focus change. Shared by the backends that learn the focused app this way.

use super::{data_file, AppIdentity, FocusUpdate};
use std::time::SystemTime;

/// Polls the focus file and reports only changes (by modification time).
#[derive(Default)]
pub(super) struct FocusFile {
    last_mtime: Option<SystemTime>,
}

impl FocusFile {
    /// The file's path, `None` without `$HOME`.
    pub(super) fn path() -> Option<std::path::PathBuf> {
        data_file("focus.json")
    }

    /// `Unchanged` until the file's mtime changes; then the app it names, or `Unknown` if it
    /// cannot be read or parsed.
    pub(super) fn poll(&mut self) -> FocusUpdate {
        let Some(path) = Self::path() else { return FocusUpdate::Unchanged };
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if mtime == self.last_mtime {
            return FocusUpdate::Unchanged;
        }
        self.last_mtime = mtime;
        match std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<AppIdentity>(&t).ok()) {
            Some(app) => FocusUpdate::Known(app),
            None => FocusUpdate::Unknown,
        }
    }
}
