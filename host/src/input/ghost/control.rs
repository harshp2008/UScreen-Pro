//! The small request files the daemon watches in `~/.local/share/uscreen/`: the route toggle
//! (`uscreen toggle-tablet`, the extension's Ctrl+Alt+P) and the pen-recorder dump request
//! (`uscreen debug-dump`), plus where the recorder's output goes.

use crate::desktop::data_file;

/// The file the extension (hotkey) and `uscreen toggle-tablet` write to flip the route.
pub fn toggle_file() -> Option<std::path::PathBuf> {
    data_file("toggle_request")
}

/// The file `uscreen debug-dump` writes to ask the daemon for the pen recorder's contents.
pub fn dump_request_file() -> Option<std::path::PathBuf> {
    data_file("dump_request")
}

/// Where the daemon writes the pen recorder's contents.
pub fn dump_file() -> Option<std::path::PathBuf> {
    data_file("pen_dump.txt")
}
