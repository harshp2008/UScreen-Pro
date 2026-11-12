//! Control channel to the `uscreen` host daemon, process status and systemd autostart.

use crate::adb::{self, ConnectedDevice};
use crate::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;

/// Snapshot polled by the UI: is the daemon alive, is its control socket answering, which version, and which devices it knows.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DaemonStatus {
    pub is_running: bool,
    pub pid: Option<u32>,
    pub control_connected: bool,
    pub host_version: String,
    pub is_dirty: bool,
    pub control_socket: String,
    pub devices: Vec<ConnectedDevice>,
}

// PLATFORM(linux): XDG_RUNTIME_DIR
/// Runtime directory of the daemon: `XDG_RUNTIME_DIR`, else `/run/user/<uid>`, else `~/.cache`, plus `uscreen`.
fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| {
            let p = PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() }));
            p.is_dir().then_some(p)
        })
        .unwrap_or_else(|| config::home_dir().join(".cache"))
        .join("uscreen")
}

/// Path of the daemon's control socket.
fn control_sock_path() -> PathBuf {
    runtime_dir().join("control.sock")
}

// PLATFORM(linux): pid file under ~/.local/share/uscreen
/// Path of the daemon's pid file.
fn pid_path() -> PathBuf {
    config::home_dir().join(".local/share/uscreen/uscreen.pid")
}

/// Sends one command to the control socket and returns the reply; an error if the socket does not exist.
async fn send_control_cmd(cmd: &str) -> Result<String, String> {
    let path = control_sock_path();
    if !path.exists() {
        return Err("Daemon control socket does not exist (daemon not running)".into());
    }
    let mut stream = UnixStream::connect(&path)
        .await
        .map_err(|e| format!("Failed to connect to control socket: {e}"))?;
    stream
        .write_all(format!("{cmd}\n").as_bytes())
        .await
        .map_err(|e| format!("Failed to send control command: {e}"))?;

    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read control response: {e}"))?;

    let trimmed = line.trim();
    match trimmed.strip_prefix("ERR ") {
        Some(err) => Err(err.to_string()),
        None => Ok(trimmed.to_string()),
    }
}

/// Pause: SIGSTOP the device's ffmpeg and evdi_helper (the daemon owns the PIDs).
#[tauri::command]
pub async fn pause_device(serial: String) -> Result<String, String> {
    send_control_cmd(&format!("FREEZE {serial}")).await
}

/// Resume: SIGCONT the same processes.
#[tauri::command]
pub async fn resume_device(serial: String) -> Result<String, String> {
    send_control_cmd(&format!("THAW {serial}")).await
}

/// Kill and respawn the ffmpeg and evdi_helper of one device (`None`: every device) so the
/// settings saved in config.toml take effect. A no-op when the daemon is not running.
///
/// Returns as soon as the restart has been requested. The UI is told through events: `pipeline_restarting`
/// right away, then one `pipeline_restarted` per slot once the new encoder has delivered a frame
/// (or `ok: false` after 25 s), then a final one with `done: true`.
pub async fn restart_pipeline(app: &AppHandle, serial: Option<&str>) -> Result<String, String> {
    if !control_sock_path().exists() {
        return Ok("daemon not running; settings apply on next start".into());
    }
    let target = serial.map(str::to_string);
    let _ = app.emit(
        "pipeline_restarting",
        PipelineEvent {
            serial: target.clone(),
            ok: true,
            live_ms: None,
            message: "restarting".into(),
            done: false,
        },
    );
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let cmd = format!("RESTART_WAIT {}", target.as_deref().unwrap_or("ALL"));
        let reply = send_control_cmd(&cmd).await;
        let mut slots = 0;
        match reply {
            Ok(text) => {
                let parsed: Option<serde_json::Value> = text
                    .strip_prefix("OK ")
                    .and_then(|j| serde_json::from_str(j).ok());
                for slot in parsed
                    .as_ref()
                    .and_then(|v| v["restarted"].as_array())
                    .into_iter()
                    .flatten()
                {
                    slots += 1;
                    let live_ms = slot["live_ms"].as_u64();
                    let _ = app.emit(
                        "pipeline_restarted",
                        PipelineEvent {
                            serial: slot["serial"].as_str().map(str::to_string),
                            ok: live_ms.is_some(),
                            live_ms,
                            message: match live_ms {
                                Some(ms) => format!("stream live after {ms} ms"),
                                None => "restarted, but no frame arrived within 25 s".into(),
                            },
                            done: false,
                        },
                    );
                }
                let _ = app.emit(
                    "pipeline_restarted",
                    PipelineEvent {
                        serial: target,
                        ok: true,
                        live_ms: None,
                        message: if slots == 0 {
                            "no active session to restart".into()
                        } else {
                            format!("{slots} pipeline(s) restarted")
                        },
                        done: true,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "pipeline_restarted",
                    PipelineEvent {
                        serial: target,
                        ok: false,
                        live_ms: None,
                        message: e,
                        done: true,
                    },
                );
            }
        }
    });
    Ok("restart requested".into())
}

/// Progress of a pipeline restart, emitted to the frontend as the daemon respawns encoders.
#[derive(Serialize, Clone, Debug)]
pub struct PipelineEvent {
    /// Device the event is about; `None` means every device.
    pub serial: Option<String>,
    pub ok: bool,
    pub live_ms: Option<u64>,
    pub message: String,
    /// Last event of one restart request.
    pub done: bool,
}

/// Stop the live session of a forgotten device. A no-op when the daemon is not running.
pub async fn forget_session(serial: &str) -> Result<String, String> {
    if !control_sock_path().exists() {
        return Ok("daemon not running; no session to stop".into());
    }
    send_control_cmd(&format!("FORGET {serial}")).await
}

/// Tauri command: restart the video pipeline of one device.
#[tauri::command]
pub async fn restart_session(app: AppHandle, serial: String) -> Result<String, String> {
    restart_pipeline(&app, Some(&serial)).await
}

/// Ask the daemon to install the bundled APK on a device. The daemon owns the queue (two at a
/// time), so the same job is shared by this button and the automatic update on reconnect. With
/// no daemon running the install happens directly.
#[tauri::command]
pub async fn queue_apk_install(serial: String) -> Result<String, String> {
    if !control_sock_path().exists() {
        return adb::install_apk(None, &serial, false).await;
    }
    send_control_cmd(&format!("INSTALL {serial}")).await
}

/// Tauri command: gather [`DaemonStatus`] from the pid file, a control-socket `PING` and the adb device list.
#[tauri::command]
pub async fn get_daemon_status() -> Result<DaemonStatus, String> {
    let pid = tokio::fs::read_to_string(pid_path())
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    let is_running = pid.is_some_and(|p| unsafe { libc::kill(p as i32, 0) == 0 });
    let control_connected = send_control_cmd("PING").await.is_ok_and(|r| r == "PONG");

    let host_version = env!("GIT_VERSION_STRING").to_string();
    let cfg = config::load().await;

    let mut devices = adb::connected_devices(&cfg).await;
    if control_connected
        && let Ok(reply) = send_control_cmd("INSTALLS").await
        && let Some(jobs) = reply.strip_prefix("OK ").and_then(|j| {
            serde_json::from_str::<std::collections::HashMap<String, adb::InstallStatus>>(j).ok()
        })
    {
        for d in &mut devices {
            d.install = jobs.get(&d.serial).cloned();
        }
    }
    if control_connected && let Ok(reply) = send_control_cmd("FROZEN").await {
        let frozen: Vec<&str> = reply
            .trim_start_matches("OK")
            .trim()
            .split(',')
            .filter(|s| !s.is_empty())
            .collect();
        for d in &mut devices {
            d.paused = frozen.contains(&d.serial.as_str());
        }
    }

    Ok(DaemonStatus {
        is_running,
        pid,
        control_connected,
        is_dirty: host_version.contains("-dirty"),
        host_version,
        control_socket: control_sock_path().to_string_lossy().into_owned(),
        devices,
    })
}

/// Name of the systemd user unit.
const UNIT: &str = "uscreen.service";

// PLATFORM(systemd): every start, stop and status of the daemon goes through `systemctl --user`
/// Runs `systemctl --user` with `args`; `None` if it could not be run.
async fn systemctl(args: &[&str]) -> Option<std::process::Output> {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .await
        .ok()
}

/// Whether the systemd user unit is installed.
async fn unit_installed() -> bool {
    systemctl(&["cat", UNIT])
        .await
        .is_some_and(|o| o.status.success())
}

/// Whether the systemd user unit is active.
async fn unit_active() -> bool {
    systemctl(&["is-active", UNIT])
        .await
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
}

// PLATFORM(linux): looks in ~/.local/bin first
/// First existing daemon binary among `~/.local/bin`, `/usr/local/bin` and `/usr/bin`.
pub(crate) fn daemon_binary() -> Option<PathBuf> {
    [
        config::home_dir().join(".local/bin/uscreen"),
        PathBuf::from("/usr/local/bin/uscreen"),
        PathBuf::from("/usr/bin/uscreen"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

/// Whether the pid in the pid file is a live process.
async fn pid_alive() -> bool {
    let pid = tokio::fs::read_to_string(pid_path())
        .await
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok());
    pid.is_some_and(|p| unsafe { libc::kill(p, 0) == 0 })
}

/// Poll until the daemon's liveness equals `want`, for at most `secs` seconds.
async fn wait_for(want: bool, secs: u64) -> bool {
    for _ in 0..secs * 5 {
        if pid_alive().await == want {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    pid_alive().await == want
}

/// The command's stderr, or `fallback` when stderr is empty.
fn failure(out: &std::process::Output, fallback: &str) -> String {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.is_empty() {
        fallback.to_string()
    } else {
        err
    }
}

/// Start through the systemd user unit when one is installed (so it is supervised the same
/// way as at login), otherwise spawn the binary detached from this GUI.
#[tauri::command]
pub async fn launch_daemon() -> Result<String, String> {
    if pid_alive().await {
        return Ok("Daemon is already running".into());
    }
    if unit_installed().await {
        let out = systemctl(&["start", UNIT])
            .await
            .ok_or("systemctl not available")?;
        if !out.status.success() {
            return Err(failure(&out, "systemctl start failed"));
        }
    } else {
        let binary =
            daemon_binary().ok_or("uscreen daemon binary not found. Run ./build-all.sh first.")?;
        Command::new(binary)
            .arg("start")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|e| format!("Failed to spawn uscreen daemon: {e}"))?;
    }
    if wait_for(true, 8).await {
        Ok("Daemon started".into())
    } else {
        Err("Daemon did not come up within 8 seconds; check `journalctl --user -u uscreen`".into())
    }
}

/// Tauri command: stop the daemon via its systemd unit when it runs under one, otherwise through `uscreen stop`. Leaves the GUI window open.
#[tauri::command]
pub async fn stop_daemon() -> Result<String, String> {
    if !pid_alive().await && !unit_active().await {
        return Ok("Daemon is not running".into());
    }
    if unit_active().await {
        let out = systemctl(&["stop", UNIT])
            .await
            .ok_or("systemctl not available")?;
        if !out.status.success() {
            return Err(failure(&out, "systemctl stop failed"));
        }
    } else {
        let binary = daemon_binary().ok_or("uscreen daemon binary not found")?;
        let out = Command::new(binary)
            .arg("stop")
            .output()
            .await
            .map_err(|e| format!("Failed to run uscreen stop: {e}"))?;
        if !out.status.success() {
            return Err(failure(&out, "uscreen stop failed"));
        }
    }
    if wait_for(false, 8).await {
        Ok("Daemon stopped".into())
    } else {
        Err("Daemon is still running after 8 seconds".into())
    }
}

// PLATFORM(systemd): autostart is the enabled state of uscreen.service
/// Tauri command: is the user service enabled at login?
#[tauri::command]
pub async fn get_autostart_status() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-enabled", "uscreen.service"])
        .output()
        .await
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled")
}

// PLATFORM(systemd): enables or disables uscreen.service for the user
/// Tauri command: enable (and start) or disable the user service.
#[tauri::command]
pub async fn set_autostart_status(enabled: bool) -> Result<bool, String> {
    let verb = if enabled { "enable" } else { "disable" };
    let out = Command::new("systemctl")
        .args(["--user", verb, "--now", "uscreen.service"])
        .output()
        .await
        .map_err(|e| format!("systemctl error: {e}"))?;
    if out.status.success() {
        return Ok(enabled);
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() {
        format!("Failed to {verb} uscreen.service")
    } else {
        err
    })
}
