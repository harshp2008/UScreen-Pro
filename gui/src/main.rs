#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

/// What one device reported about itself and per-device overrides.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
struct TabletProfile {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub width: u32,
    pub height: u32,
    pub width_mm: u32,
    pub height_mm: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_scale: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ten_bit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_apps: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(flatten)]
    pub extra: toml::value::Table,
}

impl TabletProfile {
    pub fn display_name<'a>(&'a self, fallback: &'a str) -> &'a str {
        if let Some(ref n) = self.nickname {
            let trimmed = n.trim();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
        if !self.label.trim().is_empty() {
            return self.label.trim();
        }
        fallback
    }

    pub fn has_display(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// Mirror of the daemon's config file (~/.config/uscreen/config.toml).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
struct FileConfig {
    encoder: String,
    fps: u32,
    bitrate: u32,
    width: u32,
    height: u32,
    quality: u32,
    stream_scale: u32,
    pen_only: bool,
    position: String,
    ten_bit: bool,
    require_token: bool,
    check_updates: bool,
    max_tablets: u32,
    wifi_address: String,
    vaapi_device: String,
    #[serde(default, alias = "devices")]
    tablets: std::collections::HashMap<String, TabletProfile>,
    pointer_handoff: bool,
    auto_resolution: bool,
    video_port: u16,
    input_port: u16,
    auto_launch_app: bool,
    max_performance_mode: bool,
    hevc_low_latency_tuning: bool,
    keep_alive: bool,
    #[serde(default)]
    locked_apps: Vec<String>,
    #[serde(default = "default_version")]
    version: String,
}

fn default_version() -> String {
    env!("GIT_VERSION_STRING").to_string()
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            encoder: "h264_nvenc".into(),
            fps: 60,
            bitrate: 20000,
            width: 2960,
            height: 1848,
            quality: DEFAULT_QUALITY,
            stream_scale: 1,
            pen_only: false,
            position: "right".into(),
            ten_bit: false,
            require_token: true,
            check_updates: true,
            max_tablets: 1,
            wifi_address: String::new(),
            vaapi_device: String::new(),
            tablets: std::collections::HashMap::new(),
            pointer_handoff: true,
            auto_resolution: true,
            video_port: 8890,
            input_port: 8891,
            auto_launch_app: true,
            max_performance_mode: false,
            hevc_low_latency_tuning: true,
            keep_alive: false,
            locked_apps: Vec::new(),
        }
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/uscreen/config.toml")
}

const MAX_BITRATE_KBPS: u32 = 60_000;
const MIN_BITRATE_KBPS: u32 = 1_000;
const MAX_FPS: u32 = 90;
const MIN_FPS: u32 = 10;
const DEFAULT_QUALITY: u32 = 18;
const MIN_QUALITY: u32 = 12;
const MAX_QUALITY: u32 = 32;

impl FileConfig {
    fn load() -> Self {
        let mut cfg: Self = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default();
        cfg.bitrate = cfg.bitrate.clamp(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS);
        cfg.fps = cfg.fps.clamp(MIN_FPS, MAX_FPS);
        cfg.quality = cfg.quality.clamp(MIN_QUALITY, MAX_QUALITY);
        cfg.stream_scale = cfg.stream_scale.clamp(1, 4);
        cfg
    }

    fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self).unwrap_or_default())
    }
}

fn autostart_enabled() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-enabled", "uscreen.service"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled")
        .unwrap_or(false)
}

fn set_autostart(on: bool) -> Result<(), String> {
    let verb = if on { "enable" } else { "disable" };
    let out = Command::new("systemctl")
        .args(["--user", verb, "--now", "uscreen.service"])
        .output()
        .map_err(|e| format!("systemctl failed: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DeviceAdbState {
    Device,
    Unauthorized,
    Offline,
    Failed(String),
}

impl Default for DeviceAdbState {
    fn default() -> Self {
        Self::Offline
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
struct ConnectedDevice {
    serial: String,
    hw_id: String,
    model: String,
    transport: String,
    adb_state: DeviceAdbState,
    state_detail: String,
    app_version: Option<String>,
    version_mismatch: bool,
}

#[derive(Default, Clone)]
struct Status {
    daemon_running: bool,
    daemon_pid: u32,
    device_connected: bool,
    device_model: String,
    connected_devices: Vec<ConnectedDevice>,
    /// -1 = evdi module not loaded, otherwise the device count
    evdi_count: i32,
    ffmpeg_ok: bool,
    adb_ok: bool,
    autostart: bool,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn pid_path() -> PathBuf {
    PathBuf::from(format!("{}/.local/share/uscreen/uscreen.pid", home()))
}

fn find_uscreen_bin() -> Option<PathBuf> {
    let installed = PathBuf::from(format!("{}/.local/bin/uscreen", home()));
    if installed.exists() {
        return Some(installed);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("uscreen");
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }
    if Command::new("uscreen").arg("--version").output().is_ok() {
        return Some(PathBuf::from("uscreen"));
    }
    None
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn find_apk_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    // 1. Check local share directory (standard user install location)
    let h = home();
    if !h.is_empty() {
        candidates.push(PathBuf::from(format!("{}/.local/share/uscreen/app-debug.apk", h)));
    }

    // 2. Check system-wide installation directory
    candidates.push(PathBuf::from("/usr/share/uscreen/app-debug.apk"));
    candidates.push(PathBuf::from("/usr/local/share/uscreen/app-debug.apk"));

    // 3. Resolve relative to the current executable's directory (traversing up to find android/app/build)
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(real_exe) = std::fs::canonicalize(exe) {
            let mut cur = real_exe.parent();
            for _ in 0..5 {
                if let Some(parent) = cur {
                    let p = parent.join("android/app/build/outputs/apk/debug/app-debug.apk");
                    candidates.push(p);
                    cur = parent.parent();
                } else {
                    break;
                }
            }
        }
    }

    // 4. Check known project paths in home directory
    if !h.is_empty() {
        candidates.push(PathBuf::from(format!("{}/Documents/GITHUB PROJECTS/UScreen/android/app/build/outputs/apk/debug/app-debug.apk", h)));
        candidates.push(PathBuf::from(format!("{}/UScreen/android/app/build/outputs/apk/debug/app-debug.apk", h)));
        candidates.push(PathBuf::from(format!("{}/src/UScreen/android/app/build/outputs/apk/debug/app-debug.apk", h)));
    }

    // 5. Check relative to current working directory
    candidates.push(PathBuf::from("android/app/build/outputs/apk/debug/app-debug.apk"));
    candidates.push(PathBuf::from("app-debug.apk"));

    candidates.into_iter().find(|p| p.is_file())
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum InstallStage {
    Idle,
    StepA_StoppingSession,
    StepB_WaitingDisplayUnmount,
    StepC_UninstallingOldApp,
    StepD_InstallingNewApk,
    StepE_RespawningSession,
    Done(Result<String, String>),
}

#[derive(Clone, Debug)]
pub enum InstallWorkerMsg {
    StageUpdate(InstallStage, String),
    LogLine(String),
    Finished(Result<String, String>),
}

#[derive(Clone, Debug)]
pub struct InstallerModalState {
    pub open: bool,
    pub target_serial: String,
    pub force_reinstall: bool,
    pub stage: InstallStage,
    pub status_text: String,
    pub logs: Vec<String>,
    pub is_running: bool,
}

impl Default for InstallerModalState {
    fn default() -> Self {
        Self {
            open: false,
            target_serial: String::new(),
            force_reinstall: false,
            stage: InstallStage::Idle,
            status_text: "Ready to update / install APK.".to_string(),
            logs: Vec::new(),
            is_running: false,
        }
    }
}

fn control_sock_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", uid)));
    runtime_dir.join("uscreen").join("control.sock")
}

fn send_daemon_control(cmd: &str) -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write};
    let sock = control_sock_path();
    if !sock.exists() {
        return Err(format!("Control socket {:?} does not exist (daemon not running)", sock));
    }
    let mut stream = std::os::unix::net::UnixStream::connect(&sock)
        .map_err(|e| format!("Failed to connect to {:?}: {}", sock, e))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    writeln!(stream, "{}", cmd).map_err(|e| format!("Failed to send to control socket: {}", e))?;
    stream.flush().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| format!("Failed to read response: {}", e))?;
    let resp = line.trim().to_string();
    if let Some(err) = resp.strip_prefix("ERR ") {
        Err(err.to_string())
    } else {
        Ok(resp)
    }
}

fn start_surgical_update(
    serial: String,
    force_reinstall: bool,
    tx: Sender<InstallWorkerMsg>,
) {
    std::thread::spawn(move || {
        let log = |s: &str| {
            let _ = tx.send(InstallWorkerMsg::LogLine(s.to_string()));
        };
        let stage = |st: InstallStage, desc: &str| {
            let _ = tx.send(InstallWorkerMsg::StageUpdate(st, desc.to_string()));
            let _ = tx.send(InstallWorkerMsg::LogLine(format!(">>> {}", desc)));
        };

        log(&format!("Starting surgical update pipeline for target device: {}", serial));
        log(&format!("Force clean reinstall mode: {}", force_reinstall));

        let apk = match find_apk_path() {
            Some(p) => p,
            None => {
                let err = "app-debug.apk not found. Build it with `./gradlew assembleDebug` or copy to ~/.local/share/uscreen/app-debug.apk".to_string();
                log(&format!("ERROR: {}", err));
                let _ = tx.send(InstallWorkerMsg::Finished(Err(err)));
                return;
            }
        };
        log(&format!("Located APK: {} ({} bytes)", apk.display(), std::fs::metadata(&apk).map(|m| m.len()).unwrap_or(0)));

        // Step A: Signal daemon to gracefully pause target device's EVDI helper & FFmpeg
        stage(InstallStage::StepA_StoppingSession, "Step A: Signaling daemon to pause target device pipeline...");
        let daemon_online = match send_daemon_control(&format!("PAUSE {}", serial)) {
            Ok(resp) => {
                log(&format!("Daemon acknowledged pause: {}", resp));
                true
            }
            Err(e) => {
                log(&format!("Daemon control socket not available ({}). Killing target helper/ffmpeg directly.", e));
                let _ = Command::new("pkill")
                    .args(["-f", "evdi_helper .*--capture-fifo"])
                    .status();
                let _ = Command::new("pkill")
                    .args(["-f", "ffmpeg.*capture.*fifo"])
                    .status();
                false
            }
        };

        // Step B: Wait for target device's virtual display to unmount cleanly
        stage(InstallStage::StepB_WaitingDisplayUnmount, "Step B: Waiting for target virtual display to unmount...");
        std::thread::sleep(Duration::from_millis(1200));
        log("Virtual display unmount grace period elapsed.");

        // Step C: Run `adb -s <serial> uninstall com.uscreen` to gracefully delete old app
        if force_reinstall {
            stage(InstallStage::StepC_UninstallingOldApp, "Step C: Executing `adb uninstall com.uscreen`...");
            match Command::new("adb").args(["-s", &serial, "uninstall", "com.uscreen"]).output() {
                Ok(out) => {
                    let out_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    let err_str = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    log(&format!("adb uninstall stdout: {}", out_str));
                    if !err_str.is_empty() {
                        log(&format!("adb uninstall stderr: {}", err_str));
                    }
                }
                Err(e) => {
                    log(&format!("Warning: failed to invoke adb uninstall: {}", e));
                }
            }
        } else {
            log("Step C: Preserving application data (fast upgrade mode, skipping adb uninstall).");
        }

        // Step D: Run `adb -s <serial> install -r app-debug.apk`
        stage(InstallStage::StepD_InstallingNewApk, "Step D: Executing `adb install -r app-debug.apk`...");
        let install_out = Command::new("adb")
            .args(["-s", &serial, "install", "-r", &apk.to_string_lossy()])
            .output();

        let install_success = match install_out {
            Ok(out) if out.status.success() => {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                log(&format!("adb install success: {}", s));
                true
            }
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
                log(&format!("adb install failed! Exit code: {:?}\nstdout: {}\nstderr: {}", out.status.code(), s, e));
                false
            }
            Err(e) => {
                log(&format!("Failed to execute adb install: {}", e));
                false
            }
        };

        if !install_success {
            let err = format!("Installation of APK failed on device {}. Check logs.", serial);
            let _ = tx.send(InstallWorkerMsg::Finished(Err(err)));
            return;
        }

        // Step E: Trigger daemon to re-spawn EVDI session and relaunch app with correct dynamic port intents
        stage(InstallStage::StepE_RespawningSession, "Step E: Signaling daemon to resume EVDI pipeline & launch app...");
        if daemon_online {
            match send_daemon_control(&format!("RESUME {}", serial)) {
                Ok(resp) => {
                    log(&format!("Daemon acknowledged resume: {}", resp));
                }
                Err(e) => {
                    log(&format!("Warning: resume command error: {}. Launching app via adb am start directly.", e));
                    let _ = Command::new("adb")
                        .args(["-s", &serial, "shell", "am", "start", "-n", "com.uscreen/.MainActivity"])
                        .output();
                }
            }
        } else {
            log("Daemon is not running. Launching UScreen app directly via adb am start...");
            let _ = Command::new("adb")
                .args(["-s", &serial, "shell", "am", "start", "-n", "com.uscreen/.MainActivity"])
                .output();
            log("Note: Start the UScreen daemon to begin display streaming.");
        }

        let success_msg = format!("Successfully installed and synced UScreen v{} on {}", env!("GIT_VERSION_STRING"), serial);
        log(&format!(">>> COMPLETE: {}", success_msg));
        let _ = tx.send(InstallWorkerMsg::Finished(Ok(success_msg)));
    });
}

#[allow(clippy::field_reassign_with_default)]
fn poll_status() -> Status {
    let mut s = Status::default();

    s.evdi_count = std::fs::read_to_string("/sys/devices/evdi/count")
        .ok()
        .and_then(|t| t.trim().parse::<i32>().ok())
        .unwrap_or(-1);
    s.ffmpeg_ok = command_exists("ffmpeg");
    s.autostart = autostart_enabled();
    s.adb_ok = command_exists("adb");

    if let Ok(pid_str) = std::fs::read_to_string(pid_path()) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if PathBuf::from(format!("/proc/{}", pid)).exists() {
                s.daemon_running = true;
                s.daemon_pid = pid;
            }
        }
    }

    if let Ok(out) = Command::new("adb").args(["devices", "-l"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut raw_devices = Vec::new();
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            let serial = parts.first().copied().unwrap_or("").to_string();
            if serial.is_empty() {
                continue;
            }
            let state_str = parts.get(1).copied().unwrap_or("unknown");
            let model = parts
                .iter()
                .find_map(|t| t.strip_prefix("model:"))
                .map(|m| m.replace('_', " "))
                .unwrap_or_else(|| serial.clone());
            let transport = if serial.contains(':') || serial.contains("._adb") {
                "Wi-Fi".to_string()
            } else {
                "USB".to_string()
            };
            let (adb_state, state_detail) = match state_str {
                "device" => (
                    DeviceAdbState::Device,
                    "Device connected and authorized. Display stream ready.".to_string(),
                ),
                "unauthorized" => (
                    DeviceAdbState::Unauthorized,
                    "RSA Fingerprint not accepted. Check your tablet screen.".to_string(),
                ),
                "offline" => (
                    DeviceAdbState::Offline,
                    "Device is offline. Reconnect the cable or restart ADB.".to_string(),
                ),
                other => (
                    DeviceAdbState::Failed(other.to_string()),
                    format!("ADB status: {}. Check device cable and permissions.", other),
                ),
            };

            let hw_id = if transport == "USB" {
                serial.clone()
            } else {
                Command::new("adb")
                    .args(["-s", &serial, "shell", "getprop", "ro.serialno"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty() && s != "unknown")
                    .unwrap_or_else(|| serial.clone())
            };

            let (app_version, version_mismatch) = if adb_state == DeviceAdbState::Device {
                let ver = Command::new("adb")
                    .args(["-s", &serial, "shell", "dumpsys package com.uscreen | grep versionName"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        let out = String::from_utf8_lossy(&o.stdout);
                        out.lines().find_map(|l| {
                            l.split("versionName=").nth(1).map(|v| v.trim().to_string())
                        })
                    });
                let mismatch = match ver.as_deref() {
                    Some(v) => v != env!("GIT_VERSION_STRING"),
                    None => true,
                };
                (ver, mismatch)
            } else {
                (None, false)
            };

            raw_devices.push(ConnectedDevice {
                serial,
                hw_id,
                model,
                transport,
                adb_state,
                state_detail,
                app_version,
                version_mismatch,
            });
        }

        // Deduplicate devices by physical hardware ID, preferring USB over Wi-Fi
        let mut deduped: std::collections::HashMap<String, ConnectedDevice> = std::collections::HashMap::new();
        for dev in raw_devices {
            deduped
                .entry(dev.hw_id.clone())
                .and_modify(|existing| {
                    if existing.transport != "USB" && dev.transport == "USB" {
                        *existing = dev.clone();
                    }
                })
                .or_insert(dev);
        }
        let mut sorted_devices: Vec<ConnectedDevice> = deduped.into_values().collect();
        sorted_devices.sort_by(|a, b| a.serial.cmp(&b.serial));
        s.connected_devices = sorted_devices;

        if let Some(first) = s.connected_devices.iter().find(|d| d.adb_state == DeviceAdbState::Device) {
            s.device_connected = true;
            s.device_model = first.model.clone();
        }
    }
    if let Ok(fakes) = std::env::var("USCREEN_FAKE_TABLET") {
        for fake in fakes.split(',').map(str::trim).filter(|f| !f.is_empty()) {
            if !s.connected_devices.iter().any(|t| t.serial == fake) {
                let model = format!("Emulated Device ({})", fake);
                s.connected_devices.push(ConnectedDevice {
                    serial: fake.to_string(),
                    hw_id: fake.to_string(),
                    model: model.clone(),
                    transport: "Emulated".to_string(),
                    adb_state: DeviceAdbState::Device,
                    state_detail: "Emulated virtual test tablet".to_string(),
                    app_version: Some(env!("GIT_VERSION_STRING").to_string()),
                    version_mismatch: false,
                });
                s.device_connected = true;
                if s.device_model.is_empty() {
                    s.device_model = model;
                }
            }
        }
    }
    s
}

fn run_system_setup() -> Result<(), String> {
    let script = "set -e; \
        echo 'options evdi initial_device_count=2' > /etc/modprobe.d/uscreen-evdi.conf; \
        printf 'evdi\nuinput\n' > /etc/modules-load.d/uscreen.conf; \
        modprobe evdi || true; modprobe uinput || true; \
        if [ \"$(cat /sys/devices/evdi/count 2>/dev/null || echo 0)\" = \"0\" ]; then echo 1 > /sys/devices/evdi/add; fi";
    let out = Command::new("pkexec")
        .args(["sh", "-c", script])
        .output()
        .map_err(|e| format!("pkexec failed to run: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Setup failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn start_daemon() -> Result<(), String> {
    // 1. Try starting via systemd user service if present
    let service_cat = Command::new("systemctl")
        .args(["--user", "cat", "uscreen.service"])
        .output();
    if let Ok(out) = service_cat {
        if out.status.success() {
            let start_out = Command::new("systemctl")
                .args(["--user", "start", "uscreen.service"])
                .output();
            if let Ok(so) = start_out {
                if so.status.success() {
                    return Ok(());
                }
            }
        }
    }

    // 2. Direct decoupled spawn with setsid() and SIGHUP disarmed
    let bin = find_uscreen_bin().ok_or("uscreen binary not found — run `make install`")?;
    let log_dir = PathBuf::from(format!("{}/.local/share/uscreen", home()));
    let _ = std::fs::create_dir_all(&log_dir);
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("daemon.log"))
        .map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(bin);
    cmd.arg("start");
    cmd.stdout(log);
    cmd.stderr(log_err);
    cmd.stdin(std::process::Stdio::null());

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }

    cmd.spawn().map_err(|e| format!("Failed to start daemon: {}", e))?;
    Ok(())
}

fn stop_daemon() -> Result<(), String> {
    // Try stopping systemd service first
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "uscreen.service"])
        .output();

    let bin = find_uscreen_bin().ok_or("uscreen binary not found")?;
    Command::new(bin)
        .arg("stop")
        .output()
        .map_err(|e| format!("Failed to stop daemon: {}", e))?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectedView {
    Global,
    Device(String),
}

struct App {
    cfg: FileConfig,
    saved_cfg: FileConfig,
    status: Arc<Mutex<Status>>,
    message: String,
    update: Arc<Mutex<Option<String>>>,
    selected_view: SelectedView,
    installer_state: InstallerModalState,
    install_tx: Sender<InstallWorkerMsg>,
    install_rx: Receiver<InstallWorkerMsg>,
}

const RELEASES_API: &str = "https://api.github.com/repos/majmichu1/UScreen/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/majmichu1/UScreen/releases/latest";

fn os_release_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_default()
        + " / " + &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
        + " (" + &std::env::var("XDG_SESSION_TYPE").unwrap_or_default() + ")"
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn version_parts(v: &str) -> (u32, u32, u32) {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn check_for_update() -> Option<String> {
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "4", "-H", "Accept: application/vnd.github+json",
               "-H", concat!("User-Agent: uscreen-gui/", env!("GIT_VERSION_STRING")), RELEASES_API])
        .output().ok()?;
    if !out.status.success() { return None; }
    let body = String::from_utf8_lossy(&out.stdout);
    let tag = body.split("\"tag_name\"").nth(1)?.split('"').nth(1)?.trim_start_matches('v').to_string();
    (version_parts(&tag) > version_parts(env!("CARGO_PKG_VERSION"))).then_some(tag)
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let cfg = FileConfig::load();
        let status = Arc::new(Mutex::new(Status::default()));
        let (install_tx, install_rx) = channel();

        let update = Arc::new(Mutex::new(None));
        if cfg.check_updates {
            let slot = update.clone();
            std::thread::spawn(move || {
                if let Some(v) = check_for_update() {
                    if let Ok(mut g) = slot.lock() { *g = Some(v); }
                }
            });
        }

        let status_bg = status.clone();
        std::thread::spawn(move || loop {
            let s = poll_status();
            if let Ok(mut guard) = status_bg.lock() {
                *guard = s;
            }
            std::thread::sleep(Duration::from_secs(2));
        });

        Self {
            saved_cfg: cfg.clone(),
            cfg,
            status,
            message: String::new(),
            update,
            selected_view: SelectedView::Global,
            installer_state: InstallerModalState::default(),
            install_tx,
            install_rx,
        }
    }

    fn apply(&mut self, restart: bool) {
        match self.cfg.save() {
            Ok(_) => {
                self.saved_cfg = self.cfg.clone();
                self.message = "Settings saved".into();
                if restart {
                    let _ = stop_daemon();
                    std::thread::sleep(Duration::from_millis(500));
                    match start_daemon() {
                        Ok(_) => self.message = "Settings saved — daemon restarted".into(),
                        Err(e) => self.message = e,
                    }
                }
            }
            Err(e) => self.message = format!("Save failed: {}", e),
        }
    }

    fn render_global_settings(&mut self, ui: &mut egui::Ui, status: &Status) {
        ui.heading(egui::RichText::new("Global Settings").size(20.0));
        ui.label(egui::RichText::new("Default settings applied to all connected devices unless overridden.").weak().size(12.0));
        ui.add_space(10.0);

        // System setup banner if needed
        let needs_setup = status.evdi_count <= 0;
        let missing_pkgs = !status.ffmpeg_ok || !status.adb_ok;
        if needs_setup || missing_pkgs {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(50, 38, 22))
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Setup needed")
                            .strong()
                            .color(egui::Color32::from_rgb(255, 180, 80)),
                    );
                    if missing_pkgs {
                        let mut pkgs = vec![];
                        if !status.ffmpeg_ok { pkgs.push("ffmpeg"); }
                        if !status.adb_ok { pkgs.push("android-tools (adb)"); }
                        ui.label(format!("Install with package manager: {}", pkgs.join(", ")));
                    }
                    if needs_setup {
                        ui.label(if status.evdi_count < 0 {
                            "The EVDI kernel module is not loaded (install evdi/evdi-dkms)."
                        } else {
                            "The virtual display device needs to be enabled (one time)."
                        });
                        if ui.button("Enable virtual display (asks for password)").clicked() {
                            match run_system_setup() {
                                Ok(_) => self.message = "System setup complete".into(),
                                Err(e) => self.message = e,
                            }
                        }
                    }
                });
            ui.add_space(10.0);
        }

        egui::Grid::new("global_settings_grid")
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                ui.label("Global Encoder");
                egui::ComboBox::from_id_salt("global_encoder")
                    .selected_text(encoder_label(&self.cfg.encoder))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.cfg.encoder, "h264_nvenc".to_string(), "NVIDIA H.264 (NVENC)");
                        ui.selectable_value(&mut self.cfg.encoder, "hevc_nvenc".to_string(), "NVIDIA HEVC (NVENC)");
                        ui.selectable_value(&mut self.cfg.encoder, "h264_vaapi".to_string(), "AMD / Intel H.264 (VAAPI)");
                        ui.selectable_value(&mut self.cfg.encoder, "hevc_vaapi".to_string(), "AMD / Intel HEVC (VAAPI)");
                        ui.selectable_value(&mut self.cfg.encoder, "libx264".to_string(), "CPU (libx264)");
                    });
                ui.end_row();

                ui.label("Quality");
                ui.vertical(|ui| {
                    let mut sharpness = (MAX_QUALITY - self.cfg.quality.clamp(MIN_QUALITY, MAX_QUALITY)) as f32;
                    let span = (MAX_QUALITY - MIN_QUALITY) as f32;
                    if ui.add(egui::Slider::new(&mut sharpness, 0.0..=span).show_value(false)).changed() {
                        self.cfg.quality = MAX_QUALITY - sharpness.round() as u32;
                    }
                    ui.label(egui::RichText::new("This, not the bitrate, sets how sharp text looks").weak().size(11.0));
                });
                ui.end_row();

                ui.label("Bitrate ceiling");
                ui.vertical(|ui| {
                    let mut mbps = self.cfg.bitrate as f32 / 1000.0;
                    if ui.add(egui::Slider::new(&mut mbps, 5.0..=60.0).suffix(" Mbps")).changed() {
                        self.cfg.bitrate = (mbps * 1000.0) as u32;
                    }
                    ui.label(egui::RichText::new("Only a cap for bursts — desktop streams well below it").weak().size(11.0));
                });
                ui.end_row();

                ui.label("Frame rate");
                egui::ComboBox::from_id_salt("global_fps")
                    .selected_text(format!("{} fps", self.cfg.fps))
                    .show_ui(ui, |ui| {
                        for f in [30u32, 60, 90] {
                            ui.selectable_value(&mut self.cfg.fps, f, format!("{} fps", f));
                        }
                    });
                ui.end_row();

                ui.label("Colour depth");
                ui.vertical(|ui| {
                    let hevc = self.cfg.encoder.contains("hevc");
                    ui.add_enabled(hevc, egui::Checkbox::new(&mut self.cfg.ten_bit, "10-bit (HEVC Main10)"));
                    ui.label(egui::RichText::new(if hevc {
                        "Smooths banding on gradients."
                    } else {
                        "Needs an HEVC encoder — H.264 is 8-bit only."
                    }).small().weak());
                });
                ui.end_row();

                ui.label("Resolution");
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.cfg.auto_resolution, "Auto (match the device)");
                    ui.horizontal(|ui| {
                        ui.add_enabled(!self.cfg.auto_resolution, egui::DragValue::new(&mut self.cfg.width).range(640..=8192).speed(8));
                        ui.label("×");
                        ui.add_enabled(!self.cfg.auto_resolution, egui::DragValue::new(&mut self.cfg.height).range(480..=8192).speed(8));
                    });
                    if self.cfg.auto_resolution {
                        ui.label(egui::RichText::new(format!("currently {} × {}", self.cfg.width, self.cfg.height)).weak().size(11.0));
                    }
                });
                ui.end_row();

                ui.label("Stream detail");
                ui.vertical(|ui| {
                    egui::ComboBox::from_id_salt("global_stream_scale")
                        .selected_text(match self.cfg.stream_scale {
                            1 => "Full — sharpest",
                            2 => "Half — lowest latency",
                            n => scale_label(n),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.cfg.stream_scale, 1, "Full — sharpest");
                            ui.selectable_value(&mut self.cfg.stream_scale, 2, "Half — lowest latency");
                        });
                    ui.label(egui::RichText::new("Half sends 1/4 the pixels, decoding faster with lower latency.").weak().size(11.0));
                });
                ui.end_row();

                ui.label("Position");
                egui::ComboBox::from_id_salt("global_position")
                    .selected_text(match self.cfg.position.as_str() {
                        "left" => "Left of the other screens",
                        "above" => "Above the other screens",
                        "below" => "Below the other screens",
                        _ => "Right of the other screens",
                    })
                    .show_ui(ui, |ui| {
                        for (v, label) in [
                            ("right", "Right of the other screens"),
                            ("left", "Left of the other screens"),
                            ("above", "Above the other screens"),
                            ("below", "Below the other screens"),
                        ] {
                            ui.selectable_value(&mut self.cfg.position, v.to_string(), label);
                        }
                    });
                ui.end_row();

                ui.label("Devices");
                ui.vertical(|ui| {
                    ui.add(egui::Slider::new(&mut self.cfg.max_tablets, 1..=4).text("at once"));
                    ui.label(egui::RichText::new("Each device becomes its own screen. Needs matching EVDI virtual count.").small().weak());
                });
                ui.end_row();

                ui.label("Stylus Mode");
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.cfg.pen_only, "Graphics tablet mode instead of display");
                    ui.label(egui::RichText::new("No video streamed: pen/touch directly drives the host display with zero latency.").weak().size(11.0));
                });
                ui.end_row();

                ui.label("Security");
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.cfg.require_token, "Require session token");
                    ui.label(egui::RichText::new("Protects loopback ports from unauthorized local access.").small().weak());
                });
                ui.end_row();

                ui.label("Updates");
                ui.checkbox(&mut self.cfg.check_updates, "Check for newer release on start");
                ui.end_row();

                ui.label("Plug & play");
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.cfg.auto_launch_app, "Open app on the device automatically");
                    ui.checkbox(&mut self.cfg.pointer_handoff, "Leave mouse cursor where pen lifted");
                    let mut auto = status.autostart;
                    if ui.checkbox(&mut auto, "Start UScreen with the desktop").changed() {
                        match set_autostart(auto) {
                            Ok(_) => {
                                self.message = if auto {
                                    "Autostart on — plugging the cable in is now enough".into()
                                } else {
                                    "Autostart off".into()
                                }
                            }
                            Err(e) => self.message = e,
                        }
                    }
                });
                ui.end_row();

                ui.label("Performance");
                ui.vertical(|ui| {
                    ui.checkbox(
                        &mut self.cfg.max_performance_mode,
                        "Max Performance Mode (Force 60FPS on Performance Profile)",
                    ).on_hover_text(
                        "Eliminates idle FPS throttling when system power profile is 'performance'.",
                    );
                });
                ui.end_row();

                ui.label("Keep-Alive Display");
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.cfg.keep_alive, "Keep virtual display active when disconnected");
                    ui.label(egui::RichText::new("Preserves virtual monitors and windows when a device disconnects; throttles capture to 1 FPS.").weak().size(11.0));
                });
                ui.end_row();

                ui.label("Wireless Connection");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.cfg.wifi_address).hint_text("e.g. 192.168.1.50:5555"));
                    ui.label(egui::RichText::new("Default IP:5555 for automatic wireless fallback when USB cable is disconnected.").weak().size(11.0));
                });
                ui.end_row();

                ui.label("App Locking (Mutter)");
                ui.vertical(|ui| {
                    let mut apps_str = self.cfg.locked_apps.join(", ");
                    if ui.add(egui::TextEdit::singleline(&mut apps_str).hint_text("e.g. org.gnome.Nautilus, google-chrome")).changed() {
                        self.cfg.locked_apps = apps_str.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    ui.label(egui::RichText::new("Allowed apps on virtual display. Non-matching windows are automatically repelled to the primary display.").weak().size(11.0));
                });
                ui.end_row();
            });
    }

    fn render_device_settings(&mut self, ui: &mut egui::Ui, serial: &str, status: &Status) {
        let conn = status.connected_devices.iter().find(|t| t.serial == serial);
        let is_connected = conn.is_some();
        let fallback_model = conn.map(|t| t.model.as_str()).unwrap_or(serial);
        let adb_state = conn.map(|t| t.adb_state.clone()).unwrap_or(DeviceAdbState::Offline);
        let status_label = match &adb_state {
            DeviceAdbState::Device => format!("Connected ({})", conn.unwrap().transport),
            DeviceAdbState::Unauthorized => "Unauthorized (Action Required)".to_string(),
            DeviceAdbState::Offline => if is_connected { "Device Offline".to_string() } else { "Saved Profile (Offline)".to_string() },
            DeviceAdbState::Failed(err) => format!("Connection Error ({})", err),
        };
        let tooltip_detail = conn.map(|t| t.state_detail.as_str()).unwrap_or("Device is offline. Connect via USB or Wi-Fi to start streaming.");

        // Snapshot global defaults before mutably borrowing device profile
        let global_encoder = self.cfg.encoder.clone();
        let global_fps = self.cfg.fps;
        let global_quality = self.cfg.quality;
        let global_bitrate = self.cfg.bitrate;
        let global_ten_bit = self.cfg.ten_bit;
        let global_stream_scale = self.cfg.stream_scale;
        let global_position = self.cfg.position.clone();

        let profile = self.cfg.tablets.entry(serial.to_string()).or_default();
        if profile.label.is_empty() && conn.is_some() {
            profile.label = fallback_model.to_string();
        }

        let display_title = profile.display_name(fallback_model).to_string();

        ui.heading(egui::RichText::new(format!("Device: {}", display_title)).size(20.0));
        ui.label(egui::RichText::new("Configure per-device parameters and overrides.").weak().size(12.0));
        ui.add_space(8.0);

        // Hardware & Identity Card
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    adb_status_dot(ui, &adb_state, &status_label, tooltip_detail);
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new("Serial:").weak().size(11.0));
                    ui.label(egui::RichText::new(serial).strong().size(11.0));
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Hardware Model:").weak().size(11.0));
                    ui.label(egui::RichText::new(if profile.label.is_empty() { fallback_model } else { &profile.label }).size(11.0));
                    if profile.has_display() {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(format!("Panel: {}×{} ({}×{} mm)", profile.width, profile.height, profile.width_mm, profile.height_mm)).weak().size(11.0));
                    }
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Custom Nickname:").strong());
                    let mut nick = profile.nickname.clone().unwrap_or_default();
                    let hint = if profile.label.is_empty() { fallback_model } else { &profile.label };
                    if ui.add(egui::TextEdit::singleline(&mut nick).hint_text(hint).desired_width(220.0)).changed() {
                        if nick.trim().is_empty() {
                            profile.nickname = None;
                        } else {
                            profile.nickname = Some(nick);
                        }
                    }
                });

                if is_connected {
                    ui.add_space(8.0);
                    let host_ver = env!("GIT_VERSION_STRING");
                    let dev_ver_opt = conn.and_then(|c| c.app_version.as_deref());
                    let mismatch = conn.map(|c| c.version_mismatch).unwrap_or(true);
                    let is_dirty = host_ver.ends_with("-dirty");

                    egui::Frame::group(ui.style())
                        .fill(if mismatch { egui::Color32::from_rgb(45, 34, 20) } else { egui::Color32::from_rgb(25, 38, 25) })
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Host Version:").strong().size(12.0));
                                ui.label(egui::RichText::new(host_ver).monospace().size(12.0));
                                if is_dirty {
                                    ui.label(egui::RichText::new("⚠ Untracked Dirty").color(egui::Color32::from_rgb(255, 180, 80)).size(11.0));
                                } else {
                                    ui.label(egui::RichText::new("🟢 Tracked").color(egui::Color32::from_rgb(140, 230, 140)).size(11.0));
                                }
                            });
                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Device Version:").strong().size(12.0));
                                if let Some(v) = dev_ver_opt {
                                    ui.label(egui::RichText::new(v).monospace().size(12.0));
                                } else {
                                    ui.label(egui::RichText::new("Not Installed").color(egui::Color32::from_rgb(255, 120, 120)).size(12.0));
                                }
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if mismatch {
                                    let btn = ui.button(
                                        egui::RichText::new(format!("⚠ Update APK to {}", host_ver))
                                            .strong()
                                            .color(egui::Color32::from_rgb(255, 200, 100)),
                                    );
                                    if btn.clicked() {
                                        self.installer_state.target_serial = serial.to_string();
                                        self.installer_state.force_reinstall = false;
                                        self.installer_state.open = true;
                                        self.installer_state.is_running = true;
                                        self.installer_state.logs.clear();
                                        self.installer_state.status_text = "Starting surgical update pipeline...".to_string();
                                        start_surgical_update(serial.to_string(), false, self.install_tx.clone());
                                    }
                                } else {
                                    ui.label(
                                        egui::RichText::new(format!("✅ Up to Date ({})", host_ver))
                                            .color(egui::Color32::from_rgb(140, 230, 140))
                                            .strong()
                                            .size(12.0),
                                    );
                                }

                                if ui.button("📲 Force Reinstall").clicked() {
                                    self.installer_state.target_serial = serial.to_string();
                                    self.installer_state.force_reinstall = true;
                                    self.installer_state.open = true;
                                }
                            });
                        });
                }
            });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label(egui::RichText::new("Per-Device Overrides").strong().size(15.0));
        ui.label(egui::RichText::new("Check 'Override' to customize a setting specifically for this device. Unchecked settings follow Global Settings.").weak().size(11.0));
        ui.add_space(8.0);

        egui::Grid::new(format!("device_grid_{}", serial))
            .num_columns(2)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                // 1. Encoder Override
                ui.label("Encoder");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.encoder.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Encoder").changed() {
                        profile.encoder = if is_overridden { Some(global_encoder.clone()) } else { None };
                    }
                    if let Some(ref mut enc) = profile.encoder {
                        egui::ComboBox::from_id_salt(format!("dev_enc_{}", serial))
                            .selected_text(encoder_label(enc))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(enc, "h264_nvenc".to_string(), "NVIDIA H.264 (NVENC)");
                                ui.selectable_value(enc, "hevc_nvenc".to_string(), "NVIDIA HEVC (NVENC)");
                                ui.selectable_value(enc, "h264_vaapi".to_string(), "AMD / Intel H.264 (VAAPI)");
                                ui.selectable_value(enc, "hevc_vaapi".to_string(), "AMD / Intel HEVC (VAAPI)");
                                ui.selectable_value(enc, "libx264".to_string(), "CPU (libx264)");
                            });
                    } else {
                        ui.label(egui::RichText::new(format!("Using Global ({})", encoder_label(&global_encoder))).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 2. Frame Rate Override
                ui.label("Frame rate");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.fps.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Frame Rate").changed() {
                        profile.fps = if is_overridden { Some(global_fps) } else { None };
                    }
                    if let Some(ref mut fps) = profile.fps {
                        egui::ComboBox::from_id_salt(format!("dev_fps_{}", serial))
                            .selected_text(format!("{} fps", fps))
                            .show_ui(ui, |ui| {
                                for f in [30u32, 60, 90] {
                                    ui.selectable_value(fps, f, format!("{} fps", f));
                                }
                            });
                    } else {
                        ui.label(egui::RichText::new(format!("Using Global ({} fps)", global_fps)).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 3. Quality Override
                ui.label("Quality");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.quality.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Quality").changed() {
                        profile.quality = if is_overridden { Some(global_quality) } else { None };
                    }
                    if let Some(ref mut q) = profile.quality {
                        let mut sharpness = (MAX_QUALITY - (*q).clamp(MIN_QUALITY, MAX_QUALITY)) as f32;
                        let span = (MAX_QUALITY - MIN_QUALITY) as f32;
                        if ui.add(egui::Slider::new(&mut sharpness, 0.0..=span).show_value(false)).changed() {
                            *q = MAX_QUALITY - sharpness.round() as u32;
                        }
                    } else {
                        ui.label(egui::RichText::new(format!("Using Global (Quality {})", global_quality)).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 4. Bitrate Ceiling Override
                ui.label("Bitrate ceiling");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.bitrate.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Bitrate Ceiling").changed() {
                        profile.bitrate = if is_overridden { Some(global_bitrate) } else { None };
                    }
                    if let Some(ref mut br) = profile.bitrate {
                        let mut mbps = *br as f32 / 1000.0;
                        if ui.add(egui::Slider::new(&mut mbps, 5.0..=60.0).suffix(" Mbps")).changed() {
                            *br = (mbps * 1000.0) as u32;
                        }
                    } else {
                        ui.label(egui::RichText::new(format!("Using Global ({} Mbps)", global_bitrate as f32 / 1000.0)).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 5. Colour Depth (10-bit) Override
                ui.label("Colour depth");
                ui.vertical(|ui| {
                    let active_enc = profile.encoder.as_ref().unwrap_or(&global_encoder);
                    let hevc = active_enc.contains("hevc");
                    let mut is_overridden = profile.ten_bit.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Colour Depth").changed() {
                        profile.ten_bit = if is_overridden { Some(global_ten_bit) } else { None };
                    }
                    if let Some(ref mut tb) = profile.ten_bit {
                        ui.add_enabled(hevc, egui::Checkbox::new(tb, "10-bit (HEVC Main10)"));
                        if !hevc {
                            ui.label(egui::RichText::new("Needs an HEVC encoder override or global HEVC").small().weak());
                        }
                    } else {
                        let global_desc = if global_ten_bit { "10-bit (HEVC Main10)" } else { "8-bit" };
                        ui.label(egui::RichText::new(format!("Using Global ({})", global_desc)).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 6. Stream Detail Override
                ui.label("Stream detail");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.stream_scale.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Stream Detail").changed() {
                        profile.stream_scale = if is_overridden { Some(global_stream_scale) } else { None };
                    }
                    if let Some(ref mut scale) = profile.stream_scale {
                        egui::ComboBox::from_id_salt(format!("dev_scale_{}", serial))
                            .selected_text(match *scale {
                                1 => "Full — sharpest",
                                2 => "Half — lowest latency",
                                n => scale_label(n),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(scale, 1, "Full — sharpest");
                                ui.selectable_value(scale, 2, "Half — lowest latency");
                            });
                    } else {
                        let global_scale_desc = match global_stream_scale {
                            1 => "Full — sharpest",
                            2 => "Half — lowest latency",
                            n => scale_label(n),
                        };
                        ui.label(egui::RichText::new(format!("Using Global ({})", global_scale_desc)).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 7. Position Override
                ui.label("Position");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.position.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Screen Position").changed() {
                        profile.position = if is_overridden { Some(global_position.clone()) } else { None };
                    }
                    if let Some(ref mut pos) = profile.position {
                        egui::ComboBox::from_id_salt(format!("dev_pos_{}", serial))
                            .selected_text(match pos.as_str() {
                                "left" => "Left of the other screens",
                                "above" => "Above the other screens",
                                "below" => "Below the other screens",
                                _ => "Right of the other screens",
                            })
                            .show_ui(ui, |ui| {
                                for (v, label) in [
                                    ("right", "Right of the other screens"),
                                    ("left", "Left of the other screens"),
                                    ("above", "Above the other screens"),
                                    ("below", "Below the other screens"),
                                ] {
                                    ui.selectable_value(pos, v.to_string(), label);
                                }
                            });
                    } else {
                        let pos_desc = match global_position.as_str() {
                            "left" => "Left of the other screens",
                            "above" => "Above the other screens",
                            "below" => "Below the other screens",
                            _ => "Right of the other screens",
                        };
                        ui.label(egui::RichText::new(format!("Using Global ({})", pos_desc)).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 8. Custom Resolution
                ui.label("Custom Resolution");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut profile.width).range(0..=8192).speed(8));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut profile.height).range(0..=8192).speed(8));
                    });
                    ui.label(egui::RichText::new("0 × 0 uses device auto-negotiated panel resolution.").weak().size(11.0));
                });
                ui.end_row();

                // 9. Wireless Connection
                ui.label("Wireless Connection");
                ui.vertical(|ui| {
                    let mut addr = profile.wifi_address.clone().unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut addr).hint_text("e.g. 192.168.1.50:5555")).changed() {
                        profile.wifi_address = if addr.trim().is_empty() { None } else { Some(addr) };
                    }
                    ui.horizontal(|ui| {
                        if is_connected && ui.small_button("Enable Wi-Fi ADB (tcpip 5555)").clicked() {
                            let s = serial.to_string();
                            std::thread::spawn(move || {
                                let _ = std::process::Command::new("adb").args(["-s", &s, "tcpip", "5555"]).output();
                            });
                        }
                    });
                    ui.label(egui::RichText::new("When USB is unplugged, UScreen seamlessly transfers the session to Wi-Fi without tearing down the virtual display.").weak().size(11.0));
                });
                ui.end_row();

                // 10. Keep-Alive Display Override
                ui.label("Keep-Alive Display");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.keep_alive.is_some();
                    if ui.checkbox(&mut is_overridden, "Override Keep-Alive").changed() {
                        profile.keep_alive = if is_overridden { Some(self.cfg.keep_alive) } else { None };
                    }
                    if let Some(ref mut ka) = profile.keep_alive {
                        ui.checkbox(ka, "Keep virtual display active when disconnected");
                        ui.label(egui::RichText::new("Preserves monitor and window layout; throttles capture to 1 FPS while away.").weak().size(11.0));
                    } else {
                        ui.label(egui::RichText::new(format!("Using Global ({})", if self.cfg.keep_alive { "Enabled" } else { "Disabled" })).weak().size(11.0));
                    }
                });
                ui.end_row();

                // 11. Application Locking Override
                ui.label("App Locking (Mutter)");
                ui.vertical(|ui| {
                    let mut is_overridden = profile.locked_apps.is_some();
                    if ui.checkbox(&mut is_overridden, "Override App Locking").changed() {
                        profile.locked_apps = if is_overridden { Some(self.cfg.locked_apps.clone()) } else { None };
                    }
                    if let Some(ref mut apps) = profile.locked_apps {
                        let mut apps_str = apps.join(", ");
                        if ui.add(egui::TextEdit::singleline(&mut apps_str).hint_text("e.g. org.gnome.Nautilus, google-chrome")).changed() {
                            *apps = apps_str.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        ui.label(egui::RichText::new("Only windows matching these application IDs will stay on this monitor. Others are repelled.").weak().size(11.0));
                    } else {
                        ui.label(egui::RichText::new(format!("Using Global ({} app{})", self.cfg.locked_apps.len(), if self.cfg.locked_apps.len() == 1 { "" } else { "s" })).weak().size(11.0));
                    }
                });
                ui.end_row();
            });

        let mut remove_profile = false;
        if !is_connected {
            ui.add_space(20.0);
            if ui.button(egui::RichText::new("Forget / Delete Saved Profile").color(egui::Color32::from_rgb(240, 100, 100))).clicked() {
                remove_profile = true;
            }
        }

        if remove_profile {
            self.cfg.tablets.remove(serial);
            self.selected_view = SelectedView::Global;
        }
    }

    fn render_installer_modal(&mut self, ctx: &egui::Context, status: &Status) {
        if !self.installer_state.open {
            return;
        }

        let mut close_modal = false;

        egui::Window::new("📲 UScreen Android App Surgical Updater")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .fixed_size([500.0, 420.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("APK Surgical Updater");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.installer_state.is_running && ui.button("✕").clicked() {
                            close_modal = true;
                        }
                    });
                });
                ui.separator();
                ui.add_space(4.0);

                // Host Version Display
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Host Version:").strong());
                    ui.label(egui::RichText::new(env!("GIT_VERSION_STRING")).monospace());
                    if env!("GIT_VERSION_STRING").ends_with("-dirty") {
                        ui.label(
                            egui::RichText::new("⚠ Untracked Dirty Build")
                                .color(egui::Color32::from_rgb(255, 180, 80))
                                .size(11.0),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("🟢 Tracked Commit")
                                .color(egui::Color32::from_rgb(140, 230, 140))
                                .size(11.0),
                        );
                    }
                });

                // APK Status
                let apk_opt = find_apk_path();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Target APK:").strong());
                    if let Some(ref p) = apk_opt {
                        let size_mb = std::fs::metadata(p).map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
                        ui.label(egui::RichText::new(format!("{} ({:.2} MB)", p.display(), size_mb)).weak().size(11.0));
                    } else {
                        ui.label(
                            egui::RichText::new("Not found! Build with `./gradlew assembleDebug`")
                                .color(egui::Color32::from_rgb(255, 120, 120))
                                .size(11.0),
                        );
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // Device Selection Combo Box (never cleared, pulled directly from status)
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Target Device:").strong());
                    let devices: Vec<_> = status.connected_devices.iter().filter(|d| d.adb_state == DeviceAdbState::Device).collect();
                    if devices.is_empty() {
                        ui.label(
                            egui::RichText::new("No authorized Android devices detected via ADB")
                                .color(egui::Color32::from_rgb(255, 140, 140))
                                .italics(),
                        );
                    } else {
                        if self.installer_state.target_serial.is_empty() || !devices.iter().any(|d| d.serial == self.installer_state.target_serial) {
                            if let Some(first) = devices.first() {
                                self.installer_state.target_serial = first.serial.clone();
                            }
                        }

                        let curr_serial = self.installer_state.target_serial.clone();
                        let curr_label = devices.iter()
                            .find(|d| d.serial == curr_serial)
                            .map(|d| format!("{} ({})", d.model, d.serial))
                            .unwrap_or_else(|| curr_serial.clone());

                        egui::ComboBox::from_id_salt("installer_target_device_combo")
                            .selected_text(curr_label)
                            .show_ui(ui, |ui| {
                                for d in &devices {
                                    let label = format!("{} ({}) [v{}]", d.model, d.serial, d.app_version.as_deref().unwrap_or("missing"));
                                    ui.selectable_value(&mut self.installer_state.target_serial, d.serial.clone(), label);
                                }
                            });
                    }
                });

                // Device Installed Version & State
                if let Some(target_dev) = status.connected_devices.iter().find(|d| d.serial == self.installer_state.target_serial) {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Installed App Version:").weak());
                        if let Some(ref ver) = target_dev.app_version {
                            let match_color = if ver == env!("GIT_VERSION_STRING") {
                                egui::Color32::from_rgb(140, 230, 140)
                            } else {
                                egui::Color32::from_rgb(255, 180, 80)
                            };
                            ui.label(egui::RichText::new(ver).color(match_color).strong());
                            if ver == env!("GIT_VERSION_STRING") {
                                ui.label(egui::RichText::new("✅ Up to Date").color(match_color).size(11.0));
                            } else {
                                ui.label(egui::RichText::new("⚠ Mismatch (Update Recommended)").color(match_color).size(11.0));
                            }
                        } else {
                            ui.label(egui::RichText::new("Not Installed").color(egui::Color32::from_rgb(255, 120, 120)).strong());
                        }
                    });
                }

                ui.add_space(6.0);
                ui.checkbox(&mut self.installer_state.force_reinstall, "Force clean reinstall (run `adb uninstall com.uscreen` before install)");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Progress Status / Active Spinner Area
                if self.installer_state.is_running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new(&self.installer_state.status_text).strong().size(13.0));
                    });
                } else {
                    let text_color = match &self.installer_state.stage {
                        InstallStage::Done(Ok(_)) => egui::Color32::from_rgb(140, 230, 140),
                        InstallStage::Done(Err(_)) => egui::Color32::from_rgb(255, 120, 120),
                        _ => egui::Color32::from_rgb(200, 200, 200),
                    };
                    ui.label(egui::RichText::new(&self.installer_state.status_text).color(text_color).strong());
                }

                ui.add_space(4.0);

                // Collapsible Log Console
                ui.collapsing("Execution & ADB Logs", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.installer_state.logs {
                                ui.monospace(egui::RichText::new(line).size(10.0));
                            }
                        });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                // Buttons
                ui.horizontal(|ui| {
                    let can_install = !self.installer_state.is_running
                        && apk_opt.is_some()
                        && !self.installer_state.target_serial.is_empty();

                    let btn_label = if self.installer_state.is_running {
                        "⏳ Installing..."
                    } else if self.installer_state.force_reinstall {
                        "🚀 Force Reinstall APK"
                    } else {
                        "🚀 Start Surgical Update"
                    };

                    let btn = ui.add_enabled(can_install, egui::Button::new(btn_label));
                    if btn.clicked() {
                        self.installer_state.is_running = true;
                        self.installer_state.logs.clear();
                        self.installer_state.status_text = "Starting surgical update pipeline...".to_string();
                        let serial = self.installer_state.target_serial.clone();
                        let force = self.installer_state.force_reinstall;
                        let tx = self.install_tx.clone();
                        start_surgical_update(serial, force, tx);
                    }

                    if !self.installer_state.is_running && ui.button("Close").clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.installer_state.open = false;
        }
    }
}

fn encoder_label(enc: &str) -> &'static str {
    match enc {
        "h264_nvenc" => "NVIDIA H.264 (NVENC)",
        "hevc_nvenc" => "NVIDIA HEVC (NVENC)",
        "h264_vaapi" | "vaapih264enc" => "AMD / Intel H.264 (VAAPI)",
        "hevc_vaapi" => "AMD / Intel HEVC (VAAPI)",
        "libx264" => "CPU (libx264)",
        _ => "Custom",
    }
}

fn scale_label(n: u32) -> &'static str {
    match n {
        3 => "Third — very soft",
        _ => "Quarter — very soft",
    }
}

fn status_dot(ui: &mut egui::Ui, on: bool, label: &str, detail: &str) {
    ui.horizontal(|ui| {
        let color = if on {
            egui::Color32::from_rgb(76, 175, 80)
        } else {
            egui::Color32::from_rgb(120, 120, 130)
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, color);
        ui.label(egui::RichText::new(label).strong());
        if !detail.is_empty() {
            ui.label(egui::RichText::new(detail).weak());
        }
    });
}

fn adb_status_dot(ui: &mut egui::Ui, state: &DeviceAdbState, label: &str, tooltip: &str) {
    let (color, icon_text) = match state {
        DeviceAdbState::Device => (egui::Color32::from_rgb(76, 175, 80), "🟢"),
        DeviceAdbState::Unauthorized => (egui::Color32::from_rgb(255, 179, 0), "⚠"),
        DeviceAdbState::Offline => (egui::Color32::from_rgb(160, 160, 170), "⚪"),
        DeviceAdbState::Failed(_) => (egui::Color32::from_rgb(239, 83, 80), "🔴"),
    };
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, color);
        let resp2 = ui.label(egui::RichText::new(format!("{} {}", icon_text, label)).strong());
        let full_resp = resp.union(resp2);
        if !tooltip.is_empty() {
            full_resp.on_hover_text(tooltip);
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain surgical installer worker updates
        while let Ok(msg) = self.install_rx.try_recv() {
            match msg {
                InstallWorkerMsg::StageUpdate(st, text) => {
                    self.installer_state.stage = st;
                    self.installer_state.status_text = text;
                }
                InstallWorkerMsg::LogLine(line) => {
                    self.installer_state.logs.push(line);
                }
                InstallWorkerMsg::Finished(res) => {
                    self.installer_state.is_running = false;
                    match &res {
                        Ok(msg) => {
                            self.installer_state.stage = InstallStage::Done(Ok(msg.clone()));
                            self.installer_state.status_text = format!("✅ {}", msg);
                            self.message = msg.clone();
                        }
                        Err(err) => {
                            self.installer_state.stage = InstallStage::Done(Err(err.clone()));
                            self.installer_state.status_text = format!("❌ {}", err);
                            self.message = err.clone();
                        }
                    }
                }
            }
        }

        if self.installer_state.is_running {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        let status = self.status.lock().map(|s| s.clone()).unwrap_or_default();

        // 1. Top Panel: Header with Daemon status & PID & Start/Stop control
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("UScreen").size(18.0).strong());
                ui.label(egui::RichText::new("USB Virtual Display").weak().size(11.0));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_text = if status.daemon_running { "Stop Daemon" } else { "Start Daemon" };
                    if ui.button(btn_text).clicked() {
                        if status.daemon_running {
                            match stop_daemon() {
                                Ok(_) => self.message = "Daemon stopped".into(),
                                Err(e) => self.message = e,
                            }
                        } else {
                            match start_daemon() {
                                Ok(_) => self.message = "Daemon starting…".into(),
                                Err(e) => self.message = e,
                            }
                        }
                    }

                    ui.add_space(10.0);
                    status_dot(
                        ui,
                        status.daemon_running,
                        if status.daemon_running { "Running" } else { "Stopped" },
                        &if status.daemon_running { format!("(PID {})", status.daemon_pid) } else { String::new() },
                    );
                });
            });
            ui.add_space(4.0);
        });

        // 2. Bottom Panel: Footer with status messages & Apply/Discard actions
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.add_space(6.0);
            let dirty = self.cfg != self.saved_cfg;
            ui.horizontal(|ui| {
                if !self.message.is_empty() {
                    ui.label(egui::RichText::new(&self.message).color(egui::Color32::from_rgb(100, 200, 255)).size(11.0));
                } else {
                    ui.label(egui::RichText::new(format!("config: {}", config_path().display())).weak().size(10.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if status.daemon_running {
                        "Apply & restart"
                    } else {
                        "Save"
                    };
                    if ui.add_enabled(dirty, egui::Button::new(label)).clicked() {
                        self.apply(status.daemon_running);
                    }
                    if dirty && ui.button("Discard").clicked() {
                        self.cfg = self.saved_cfg.clone();
                    }
                });
            });
            ui.add_space(6.0);
        });

        // 3. Left Navigation Sidebar
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(220.0)
            .min_width(180.0)
            .max_width(320.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(8.0);
                    // Section 1: Navigation
                    ui.label(egui::RichText::new("NAVIGATION").strong().size(10.0).color(egui::Color32::from_rgb(140, 140, 150)));
                    ui.add_space(4.0);
                    let is_global = self.selected_view == SelectedView::Global;
                    if ui.add_sized(
                        egui::vec2(ui.available_width(), 26.0),
                        egui::Button::selectable(is_global, "🌐  Global Settings"),
                    ).clicked() {
                        self.selected_view = SelectedView::Global;
                    }

                    ui.add_space(14.0);

                    // Section 2: Devices
                    ui.label(egui::RichText::new("DEVICES").strong().size(10.0).color(egui::Color32::from_rgb(140, 140, 150)));
                    ui.add_space(4.0);

                    struct DeviceItem {
                        serial: String,
                        display_name: String,
                        transport: String,
                        adb_state: DeviceAdbState,
                        tooltip: String,
                    }
                    let mut device_list: Vec<DeviceItem> = Vec::new();

                    for cd in &status.connected_devices {
                        let disp = if let Some(p) = self.cfg.tablets.get(&cd.serial) {
                            p.display_name(&cd.model).to_string()
                        } else {
                            cd.model.clone()
                        };
                        device_list.push(DeviceItem {
                            serial: cd.serial.clone(),
                            display_name: disp,
                            transport: cd.transport.clone(),
                            adb_state: cd.adb_state.clone(),
                            tooltip: cd.state_detail.clone(),
                        });
                    }
                    for (serial, p) in &self.cfg.tablets {
                        if !device_list.iter().any(|d| &d.serial == serial) {
                            device_list.push(DeviceItem {
                                serial: serial.clone(),
                                display_name: p.display_name(serial).to_string(),
                                transport: "Saved".to_string(),
                                adb_state: DeviceAdbState::Offline,
                                tooltip: "Device offline. Connect via USB or Wi-Fi to start streaming.".to_string(),
                            });
                        }
                    }

                    if device_list.is_empty() {
                        ui.label(egui::RichText::new("No devices detected").weak().size(11.0));
                    } else {
                        for dev in device_list {
                            let is_selected = match &self.selected_view {
                                SelectedView::Device(s) => s == &dev.serial,
                                _ => false,
                            };
                            let (dot, text_color) = match dev.adb_state {
                                DeviceAdbState::Device => ("🟢", egui::Color32::from_rgb(180, 230, 180)),
                                DeviceAdbState::Unauthorized => ("⚠", egui::Color32::from_rgb(255, 215, 100)),
                                DeviceAdbState::Offline => ("⚪", egui::Color32::from_rgb(160, 160, 170)),
                                DeviceAdbState::Failed(_) => ("🔴", egui::Color32::from_rgb(255, 140, 140)),
                            };
                            let item_text = format!("{} {} ({})", dot, dev.display_name, dev.transport);
                            let mut rich = egui::RichText::new(item_text).size(12.0).color(text_color);
                            if is_selected {
                                rich = rich.strong();
                            }
                            let btn = ui.add_sized(
                                egui::vec2(ui.available_width(), 26.0),
                                egui::Button::selectable(is_selected, rich),
                            ).on_hover_text(&dev.tooltip);
                            if btn.clicked() {
                                self.selected_view = SelectedView::Device(dev.serial.clone());
                            }
                        }
                    }

                    ui.add_space(8.0);
                    let target_serial = match &self.selected_view {
                        SelectedView::Device(s) => Some(s.clone()),
                        _ => status.connected_devices.iter().find(|d| d.adb_state == DeviceAdbState::Device).map(|d| d.serial.clone()),
                    };
                    let apk_opt = find_apk_path();
                    let tooltip = match (&target_serial, &apk_opt) {
                        (Some(s), Some(p)) => format!("Push {} to {}", p.display(), s),
                        (Some(s), None) => format!("Warning: APK not found. Cannot push to {}", s),
                        (None, Some(p)) => format!("Found {}. Connect a device via USB to install.", p.display()),
                        (None, None) => "Warning: app-debug.apk not found and no active Android device connected.".to_string(),
                    };
                    if ui.add_sized(
                        egui::vec2(ui.available_width(), 26.0),
                        egui::Button::new("📲  Install Android App"),
                    ).on_hover_text(tooltip).clicked() {
                        self.installer_state.target_serial = target_serial.unwrap_or_default();
                        self.installer_state.force_reinstall = false;
                        self.installer_state.open = true;
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Utility links
                    ui.horizontal(|ui| {
                        if ui.small_button("Compatibility").clicked() {
                            let body = format!(
                                "Result: \n\nDistribution and desktop: {}\nGPU and encoder: {}\nDevice, Android, stylus: {}\nUScreen version: {}\n\nLatency line from the log (optional):\n\nNotes:\n",
                                os_release_name(), self.cfg.encoder, status.device_model, env!("GIT_VERSION_STRING"));
                            let url = format!(
                                "https://github.com/majmichu1/UScreen/issues/new?template=compatibility.yml&title={}&body={}",
                                urlencode("Compatibility: "), urlencode(&body));
                            let _ = Command::new("xdg-open").arg(url).spawn();
                        }
                        if ui.small_button("GitHub").clicked() {
                            let _ = Command::new("xdg-open").arg("https://github.com/majmichu1/UScreen").spawn();
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Host Version:").weak().size(11.0));
                        ui.label(egui::RichText::new(env!("GIT_VERSION_STRING")).monospace().size(11.0));
                        if env!("GIT_VERSION_STRING").ends_with("-dirty") {
                            ui.label(
                                egui::RichText::new("(dirty)")
                                    .color(egui::Color32::from_rgb(255, 180, 80))
                                    .size(10.0),
                            );
                        }
                    });
                    if let Some(v) = self.update.lock().ok().and_then(|g| g.clone()) {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("v{} available", v)).strong().size(11.0));
                            if ui.link("view").clicked() {
                                let _ = Command::new("xdg-open").arg(RELEASES_PAGE).spawn();
                            }
                        });
                    }
                    ui.add_space(10.0);
                });
            });

        // 4. Central Panel: Device or Global Settings
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(6.0);
                match &self.selected_view {
                    SelectedView::Global => {
                        self.render_global_settings(ui, &status);
                    }
                    SelectedView::Device(serial) => {
                        let serial = serial.clone();
                        self.render_device_settings(ui, &serial, &status);
                    }
                }
                ui.add_space(16.0);
            });
        });

        // 5. Foreground Installer Modal
        self.render_installer_modal(ctx, &status);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("uscreen-gui {}", env!("GIT_VERSION_STRING"));
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 620.0])
            .with_min_inner_size([600.0, 480.0])
            .with_app_id("uscreen")
            .with_icon(egui::IconData {
                rgba: include_bytes!("../../packaging/icons/uscreen-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "UScreen",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_apk_path_resolves() {
        let apk = find_apk_path();
        println!("Resolved APK path: {:?}", apk);
        assert!(apk.is_some(), "find_apk_path() must locate app-debug.apk");
        assert!(apk.unwrap().exists());
    }

    #[test]
    fn test_find_apk_path_from_root_dir() {
        let orig = std::env::current_dir().unwrap();
        let _ = std::env::set_current_dir("/");
        let apk = find_apk_path();
        let _ = std::env::set_current_dir(orig);
        println!("Resolved APK path from /: {:?}", apk);
        assert!(apk.is_some(), "find_apk_path() must locate app-debug.apk even when cwd is /");
        assert!(apk.unwrap().exists());
    }

    #[test]
    fn test_version_mismatch_logic() {
        let host = env!("GIT_VERSION_STRING");
        assert!(!host.is_empty());

        // Older or different version
        let dev_old = Some("1.3.0");
        let mismatch_old = match dev_old {
            Some(v) => v != host,
            None => true,
        };
        assert!(mismatch_old, "Older version must trigger mismatch");

        // Missing app
        let dev_missing: Option<&str> = None;
        let mismatch_missing = match dev_missing {
            Some(v) => v != host,
            None => true,
        };
        assert!(mismatch_missing, "Missing app must trigger mismatch (needs install)");

        // Matching version
        let dev_match = Some(host);
        let mismatch_same = match dev_match {
            Some(v) => v != host,
            None => true,
        };
        assert!(!mismatch_same, "Matching version must not trigger mismatch");
    }

    #[test]
    fn test_installer_modal_state_lifecycle() {
        let mut state = InstallerModalState::default();
        assert!(!state.open);
        assert!(!state.is_running);
        assert_eq!(state.stage, InstallStage::Idle);

        state.open = true;
        state.target_serial = "TEST_DEVICE".to_string();
        state.is_running = true;
        state.stage = InstallStage::StepA_StoppingSession;
        state.logs.push("Step A started".to_string());

        assert!(state.open);
        assert!(state.is_running);
        assert_eq!(state.logs.len(), 1);
    }
}
