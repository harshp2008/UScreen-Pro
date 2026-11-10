//! The desktop app's `adb` layer: shared types and the process wrapper (`run`), and the submodules that do the work.
//!
//! * `devices` - enumerating devices and merging a tablet's USB and Wi-Fi entries
//! * `install` - installing the bundled APK and launching the app
//! * `intents` - developer options, Wireless debugging and Identify
//!
//! Everything public in the submodules is re-exported here, so callers keep using `crate::adb::name`.
//! Tauri commands are registered by `lib.rs` through these re-exports.

use crate::config::{self, AppConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

mod devices;
mod install;
mod intents;
pub use devices::*;
pub use install::*;
pub use intents::*;

/// A device as the GUI shows it: one logical tablet, merged from its USB and Wi-Fi adb entries.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConnectedDevice {
    pub serial: String,
    pub model: String,
    pub state: String,
    /// "USB", "Wi-Fi" or "Disconnected"
    pub transport: String,
    pub paused: bool,
    pub device_version: Option<String>,
    /// Has a `[tablets.<serial>]` entry in config.toml. Unpaired devices are "new devices".
    pub paired: bool,
    /// State of this device's job in the daemon's APK install queue, when it has one.
    pub install: Option<InstallStatus>,
}

/// One job of the daemon's install queue: `queued`, `installing`, `done` or `failed`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstallStatus {
    pub state: String,
    pub detail: String,
}

/// One line of `adb devices -l`, before USB/Wi-Fi entries of the same tablet are merged.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdbEntry {
    pub serial: String,
    pub state: String,
    pub model: String,
    pub is_wifi: bool,
}

/// Run `adb <args>` with a hard timeout. `serial` adds `-s <serial>`.
pub async fn run(serial: Option<&str>, args: &[&str], timeout: Duration) -> Result<Output, String> {
    let mut cmd = Command::new("adb");
    if let Some(s) = serial.filter(|s| !s.is_empty()) {
        cmd.args(["-s", s]);
    }
    cmd.args(args).kill_on_drop(true);
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(format!("Failed to run adb: {e}")),
        Err(_) => Err(format!("adb {} timed out", args.join(" "))),
    }
}

/// Trimmed stdout of a finished command.
pub fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// stdout and stderr joined for error messages; adb reports some failures on stdout and others on stderr.
pub fn combined_of(out: &Output) -> String {
    let so = stdout_of(out);
    let se = String::from_utf8_lossy(&out.stderr).trim().to_string();
    match (so.is_empty(), se.is_empty()) {
        (_, true) => so,
        (true, false) => se,
        (false, false) => format!("{so}\n{se}"),
    }
}

/// One step of the wizard's USB work, shown verbatim on the wizard screens.
#[derive(Serialize, Clone, Debug)]
pub struct ProgressEvent {
    pub serial: Option<String>,
    pub message: String,
}

/// Tell the frontend what is happening right now (`install_progress` or `pair_progress`).
pub fn progress(app: Option<&AppHandle>, event: &str, serial: Option<&str>, message: &str) {
    if let Some(app) = app {
        let _ = app.emit(
            event,
            ProgressEvent {
                serial: serial.map(str::to_string),
                message: message.to_string(),
            },
        );
    }
}

/// Every adb call in the install path is pinned to one device with `-s`; an empty serial would make
/// `run` drop the flag and hit whichever device adb picks, so it is refused outright.
pub(crate) fn require_serial(serial: &str) -> Result<(), String> {
    if serial.trim().is_empty() {
        Err("No device serial given; refusing to run adb without -s".into())
    } else {
        Ok(())
    }
}

/// Tauri command: `adb kill-server` then `start-server`, the standard cure for a wedged adb daemon.
#[tauri::command]
pub async fn restart_adb_server() -> Result<String, String> {
    let _ = run(None, &["kill-server"], Duration::from_secs(5)).await;
    let out = run(None, &["start-server"], Duration::from_secs(10)).await?;
    Ok(combined_of(&out))
}

/// Identity of a device reached over Wi-Fi.
#[derive(Serialize, Clone, Debug)]
pub struct WirelessDeviceInfo {
    /// Hardware serial (`ro.serialno`), the key device profiles are stored under.
    pub serial: String,
    pub model: String,
}

/// Identify a device reached over `ip:port` by its hardware serial and model.
#[tauri::command]
pub async fn wireless_device_info(endpoint: String) -> Result<WirelessDeviceInfo, String> {
    let serial = ro_serial(&endpoint)
        .await
        .unwrap_or_else(|| endpoint.clone());
    let model = run(
        Some(&endpoint),
        &["shell", "getprop", "ro.product.model"],
        Duration::from_secs(3),
    )
    .await
    .map(|o| stdout_of(&o))
    .unwrap_or_default();
    Ok(WirelessDeviceInfo {
        serial,
        model: if model.is_empty() { endpoint } else { model },
    })
}

/// Tests that `require_serial` refuses an empty serial.
#[cfg(test)]
mod tests {
    use super::*;

    /// An empty or blank serial is refused.
    #[test]
    fn empty_serial_is_refused() {
        assert!(require_serial("").is_err());
        assert!(require_serial("   ").is_err());
        assert!(require_serial("PHONE_SERIAL").is_ok());
    }
}
