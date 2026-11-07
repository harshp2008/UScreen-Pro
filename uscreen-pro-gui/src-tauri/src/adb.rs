//! Thin wrappers around the `adb` CLI: device enumeration, APK install, settings intents.

use crate::config::{self, AppConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

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

pub fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn combined_of(out: &Output) -> String {
    let so = stdout_of(out);
    let se = String::from_utf8_lossy(&out.stderr).trim().to_string();
    match (so.is_empty(), se.is_empty()) {
        (_, true) => so,
        (true, false) => se,
        (false, false) => format!("{so}\n{se}"),
    }
}

pub async fn list_entries() -> Vec<AdbEntry> {
    let Ok(out) = run(None, &["devices", "-l"], Duration::from_millis(2500)).await else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            let serial = parts[0].to_string();
            let model = parts[2..]
                .iter()
                .find_map(|p| p.strip_prefix("model:"))
                .map(|m| m.replace('_', " "))
                .unwrap_or_else(|| serial.clone());
            Some(AdbEntry {
                is_wifi: serial.contains(':') || serial.contains("._adb-tls-connect"),
                state: parts[1].to_string(),
                serial,
                model,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn list_adb_devices() -> Vec<AdbEntry> {
    list_entries().await
}

fn norm_model(m: &str) -> String {
    m.trim().to_uppercase().replace('-', " ")
}

struct Group {
    model: String,
    primary_serial: String,
    has_usb: bool,
    has_wifi: bool,
    state: String,
    all_serials: Vec<String>,
}

pub(crate) fn norm_label(m: &str) -> String {
    norm_model(m)
}

pub(crate) async fn ro_serial(serial: &str) -> Option<String> {
    let out = run(Some(serial), &["shell", "getprop", "ro.serialno"], Duration::from_millis(800))
        .await
        .ok()?;
    let s = stdout_of(&out);
    (!s.is_empty() && s != "unknown").then_some(s)
}

async fn installed_app_version(serials: &[String]) -> Option<String> {
    for s in serials {
        let Ok(out) = run(
            Some(s),
            &["shell", "dumpsys", "package", "com.uscreen"],
            Duration::from_millis(800),
        )
        .await
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let version = text
            .lines()
            .find_map(|l| l.split("versionName=").nth(1))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if version.is_some() {
            return version;
        }
    }
    None
}

/// Merge a tablet's USB and Wi-Fi ADB entries into one device, then append saved-but-offline profiles.
pub async fn connected_devices(cfg: &AppConfig) -> Vec<ConnectedDevice> {
    let mut groups: Vec<Group> = Vec::new();

    for entry in list_entries().await {
        let ro = if entry.is_wifi && entry.state == "device" {
            ro_serial(&entry.serial).await
        } else {
            None
        };
        let norm = norm_model(&entry.model);
        let found = groups.iter().position(|g| {
            (!norm.is_empty() && norm == norm_model(&g.model))
                || ro
                    .as_ref()
                    .is_some_and(|r| g.primary_serial == *r || g.all_serials.contains(r))
                || (!entry.is_wifi && g.all_serials.contains(&entry.serial))
        });

        match found {
            Some(idx) => {
                let g = &mut groups[idx];
                g.all_serials.push(entry.serial.clone());
                if entry.is_wifi {
                    g.has_wifi = true;
                    if g.primary_serial.contains(':')
                        && let Some(r) = ro
                    {
                        g.primary_serial = r;
                    }
                } else {
                    g.has_usb = true;
                    g.primary_serial = entry.serial.clone();
                }
                if entry.state == "device" {
                    g.state = "device".into();
                }
            }
            None => groups.push(Group {
                primary_serial: if entry.is_wifi {
                    ro.unwrap_or_else(|| entry.serial.clone())
                } else {
                    entry.serial.clone()
                },
                model: entry.model,
                has_usb: !entry.is_wifi,
                has_wifi: entry.is_wifi,
                state: entry.state,
                all_serials: vec![entry.serial],
            }),
        }
    }

    let mut devices = Vec::new();
    for g in groups {
        let prefers_wifi = cfg
            .tablets
            .get(&g.primary_serial)
            .and_then(|p| p.preferred_transport.as_deref())
            == Some("wifi");
        let transport = match (g.has_usb, g.has_wifi) {
            (true, true) if prefers_wifi => "Wi-Fi",
            (true, _) => "USB",
            (false, true) => "Wi-Fi",
            (false, false) => "Disconnected",
        };
        let device_version = if g.state == "device" {
            installed_app_version(&g.all_serials).await
        } else {
            None
        };
        let paired = g.all_serials.iter().any(|s| cfg.tablets.contains_key(s))
            || cfg.tablets.contains_key(&g.primary_serial);
        devices.push(ConnectedDevice {
            serial: g.primary_serial,
            model: g.model,
            state: g.state,
            transport: transport.into(),
            paused: false,
            device_version,
            paired,
            install: None,
        });
    }

    for (saved_serial, profile) in &cfg.tablets {
        // Legacy profiles were keyed by "ip:port".
        if saved_serial.contains(':') {
            continue;
        }
        let label = norm_model(&profile.label);
        let present = devices
            .iter()
            .any(|d| d.serial == *saved_serial || (!label.is_empty() && norm_model(&d.model) == label));
        if !present {
            devices.push(ConnectedDevice {
                serial: saved_serial.clone(),
                model: if profile.label.is_empty() {
                    saved_serial.clone()
                } else {
                    profile.label.clone()
                },
                state: "offline".into(),
                transport: "Disconnected".into(),
                paused: false,
                device_version: profile.app_version.clone(),
                paired: true,
                install: None,
            });
        }
    }

    devices.sort_by(|a, b| {
        let (a_on, b_on) = (a.transport != "Disconnected", b.transport != "Disconnected");
        b_on.cmp(&a_on).then_with(|| a.model.cmp(&b.model))
    });
    devices
}

/// Candidate locations of the built APK; the most recently modified one wins so a stale
/// copy in `~/.local/share/uscreen` can never shadow a fresh build.
fn find_apk_path() -> Option<PathBuf> {
    let home = config::home_dir();
    let mut candidates = vec![
        home.join(".local/share/uscreen/app-debug.apk"),
        PathBuf::from("/usr/share/uscreen/app-debug.apk"),
        PathBuf::from("/usr/local/share/uscreen/app-debug.apk"),
    ];
    if let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) {
        for dir in exe.ancestors().skip(1).take(5) {
            candidates.push(dir.join("dist/app-debug.apk"));
            candidates.push(dir.join("android/app/build/outputs/apk/debug/app-debug.apk"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors().take(4) {
            candidates.push(dir.join("dist/app-debug.apk"));
        }
    }
    candidates
        .into_iter()
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((modified, p))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
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
        let _ = app.emit(event, ProgressEvent { serial: serial.map(str::to_string), message: message.to_string() });
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

/// Install the bundled APK on exactly `serial`. With `replace_existing` an already installed copy is
/// removed first (the wizard does this so an APK signed with another key, e.g. a release of the
/// original project, cannot block the install); the queue/update path keeps `-r` so app data survives.
pub async fn install_apk(app: Option<&AppHandle>, serial: &str, replace_existing: bool) -> Result<String, String> {
    require_serial(serial)?;
    let apk = find_apk_path()
        .ok_or("Could not find app-debug.apk. Run ./build-all.sh first.".to_string())?;
    let apk = apk.to_string_lossy().into_owned();
    let ev = "install_progress";

    if replace_existing {
        let installed = run(Some(serial), &["shell", "pm", "path", "com.uscreen"], Duration::from_secs(8))
            .await
            .map(|o| stdout_of(&o).contains("package:"))
            .unwrap_or(false);
        if installed {
            progress(app, ev, Some(serial), "Found old APK");
            progress(app, ev, Some(serial), "Removing old APK");
            let out = run(Some(serial), &["uninstall", "com.uscreen"], Duration::from_secs(30)).await?;
            let text = combined_of(&out);
            if !text.contains("Success") {
                return Err(format!("Could not remove the old APK: {text}"));
            }
        }
    }
    progress(app, ev, Some(serial), "Installing new APK");
    let out = run(Some(serial), &["install", "-r", "-d", &apk], Duration::from_secs(120)).await?;
    let text = combined_of(&out);
    if text.contains("Success") {
        progress(app, ev, Some(serial), "APK installed");
        Ok("APK installed".into())
    } else {
        Err(format!("Install failed: {text}"))
    }
}

#[tauri::command]
pub async fn install_apk_on_device(app: AppHandle, serial: String) -> Result<String, String> {
    install_apk(Some(&app), &serial, true).await
}

/// `am start` exits 0 even when the activity is missing, so inspect its output.
async fn start_intent(serial: Option<&str>, action: &str) -> Result<(), String> {
    let out = run(
        serial,
        &["shell", "am", "start", "-a", action],
        Duration::from_secs(8),
    )
    .await?;
    let text = combined_of(&out);
    if !out.status.success() || text.contains("Error") || text.contains("error:") {
        Err(text)
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn open_developer_options(serial: String) -> Result<String, String> {
    require_serial(&serial)?;
    start_intent(Some(&serial), "android.settings.APPLICATION_DEVELOPMENT_SETTINGS")
        .await
        .map(|_| "Opened Developer Options on the device".to_string())
}

#[tauri::command]
pub async fn open_wireless_debugging_settings(serial: String) -> Result<String, String> {
    require_serial(&serial)?;
    // The Wireless Debugging page has no public action on stock Android, so some builds reject
    // this intent; Developer Options is the fallback.
    if start_intent(Some(&serial), "android.settings.WIRELESS_DEBUGGING_SETTINGS").await.is_ok() {
        return Ok("Opened Wireless Debugging".into());
    }
    start_intent(Some(&serial), "android.settings.APPLICATION_DEVELOPMENT_SETTINGS")
        .await
        .map(|_| "Opened Developer Options; choose Wireless debugging".to_string())
}

/// Make one specific device announce itself so the user can tell which one the wizard is about to
/// set up: a notification plus a short buzz, both pinned to `serial` with `-s`. The notification is
/// what proves it worked; the vibration is best effort (tablets without a motor, older Android).
#[tauri::command]
pub async fn identify_device(serial: String) -> Result<String, String> {
    require_serial(&serial)?;
    let out = run(
        Some(&serial),
        &[
            "shell", "cmd", "notification", "post", "-S", "bigtext", "-t", "UScreen Pro",
            "uscreen-identify", "This is your target device!",
        ],
        Duration::from_secs(8),
    )
    .await?;
    let text = combined_of(&out);
    let notified = out.status.success() && !text.to_lowercase().contains("error") && !text.contains("Unknown command");
    let buzzed = run(
        Some(&serial),
        &["shell", "cmd", "vibrator_manager", "synced", "-f", "oneshot", "500", "255"],
        Duration::from_secs(5),
    )
    .await
    .map(|o| o.status.success() && combined_of(&o).is_empty())
    .unwrap_or(false);
    match (notified, buzzed) {
        (true, true) => Ok("Notification sent and the device vibrated".into()),
        (true, false) => Ok("Notification sent to the device".into()),
        (false, true) => Ok("The device vibrated".into()),
        (false, false) => Err(format!("The device did not react: {text}")),
    }
}

#[tauri::command]
pub async fn restart_adb_server() -> Result<String, String> {
    let _ = run(None, &["kill-server"], Duration::from_secs(5)).await;
    let out = run(None, &["start-server"], Duration::from_secs(10)).await?;
    Ok(combined_of(&out))
}

#[derive(Serialize, Clone, Debug)]
pub struct WirelessDeviceInfo {
    /// Hardware serial (`ro.serialno`), the key device profiles are stored under.
    pub serial: String,
    pub model: String,
}

/// Identify a device reached over `ip:port` by its hardware serial and model.
#[tauri::command]
pub async fn wireless_device_info(endpoint: String) -> Result<WirelessDeviceInfo, String> {
    let serial = ro_serial(&endpoint).await.unwrap_or_else(|| endpoint.clone());
    let model = run(Some(&endpoint), &["shell", "getprop", "ro.product.model"], Duration::from_secs(3))
        .await
        .map(|o| stdout_of(&o))
        .unwrap_or_default();
    Ok(WirelessDeviceInfo { serial, model: if model.is_empty() { endpoint } else { model } })
}

#[cfg(test)]
mod serial_tests {
    use super::*;

    #[test]
    fn empty_serial_is_refused() {
        assert!(require_serial("").is_err());
        assert!(require_serial("   ").is_err());
        assert!(require_serial("PHONE_SERIAL").is_ok());
    }

    #[tokio::test]
    async fn serial_commands_refuse_to_run_without_a_target() {
        assert!(identify_device(String::new()).await.is_err());
        assert!(open_developer_options(String::new()).await.is_err());
        assert!(open_wireless_debugging_settings("  ".into()).await.is_err());
    }
}
