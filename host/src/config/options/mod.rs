//! Per-device options that follow the "global default, per-device override" rule and are kept
//! live: Touchpad mode, immersive mode, battery saver on disconnect and the idle-dim curves.
//!
//! Every tablet slot owns one [`DeviceOptions`] value in a small registry. It is recomputed from
//! `config.toml` whenever the file changes (a 500 ms mtime poll, so a GUI save reaches the tablet
//! at once without restarting anything) and whenever the tablet itself pushes a setting. The input
//! WebSocket forwards every change to the app as a `config_update` message, the daemon's gate
//! stops the video pipeline while Touchpad mode is on, and the idle task below turns the pointer
//! position into `dim_screen` / `undim_screen` messages.
//!
//! Layout:
//! * `device` - `DeviceOptions`: the effective settings of one tablet (global default plus its override) and what they imply
//! * `overrides` - applying one setting changed on the tablet to its profile in config.toml
//! * `registry` - the live per-tablet registry: slots, watchers that republish options when config.toml changes, and the accessors the rest of the host uses
//! * `idle` - the idle-dim state machine that fades the tablet's screen when it is not being used

mod device;
mod idle;
mod overrides;
mod registry;
pub use device::*;
pub use idle::*;
use overrides::*;
pub use registry::*;

use crate::config::{resolve_curve, FileConfig, InputMode, TabletProfile};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{watch, Notify};
use tracing::{info, warn};

/// Mode forced from the command line (`--pen-only`); wins over the global config value but not
/// over a device's own override.
static CLI_MODE: OnceLock<InputMode> = OnceLock::new();

/// Make the command line's mode the global default for this run.
pub fn set_cli_mode(mode: InputMode) {
    let _ = CLI_MODE.set(mode);
}

/// Tests for resolving global and per-device settings into `DeviceOptions`, and for the idle-dim timer.
#[cfg(test)]
mod tests;
