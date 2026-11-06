//! Control channel to the `uscreen` host daemon, process status and systemd autostart.

use crate::adb::{self, ConnectedDevice};
use crate::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;

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

fn control_sock_path() -> PathBuf {
    runtime_dir().join("control.sock")
}

fn pid_path() -> PathBuf {
    config::home_dir().join(".local/share/uscreen/uscreen.pid")
}

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

#[tauri::command]
pub async fn pause_device(serial: String) -> Result<String, String> {
    send_control_cmd(&format!("PAUSE {serial}")).await
}

#[tauri::command]
pub async fn resume_device(serial: String) -> Result<String, String> {
    send_control_cmd(&format!("RESUME {serial}")).await
}

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

    Ok(DaemonStatus {
        is_running,
        pid,
        control_connected,
        is_dirty: host_version.contains("-dirty"),
        host_version,
        control_socket: control_sock_path().to_string_lossy().into_owned(),
        devices: adb::connected_devices(&cfg).await,
    })
}

const UNIT: &str = "uscreen.service";

async fn systemctl(args: &[&str]) -> Option<std::process::Output> {
    Command::new("systemctl").arg("--user").args(args).output().await.ok()
}

async fn unit_installed() -> bool {
    systemctl(&["cat", UNIT]).await.is_some_and(|o| o.status.success())
}

async fn unit_active() -> bool {
    systemctl(&["is-active", UNIT])
        .await
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
}

fn daemon_binary() -> Option<PathBuf> {
    [
        config::home_dir().join(".local/bin/uscreen"),
        PathBuf::from("/usr/local/bin/uscreen"),
        PathBuf::from("/usr/bin/uscreen"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

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

fn failure(out: &std::process::Output, fallback: &str) -> String {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.is_empty() { fallback.to_string() } else { err }
}

/// Start through the systemd user unit when one is installed (so it is supervised the same
/// way as at login), otherwise spawn the binary detached from this GUI.
#[tauri::command]
pub async fn launch_daemon() -> Result<String, String> {
    if pid_alive().await {
        return Ok("Daemon is already running".into());
    }
    if unit_installed().await {
        let out = systemctl(&["start", UNIT]).await.ok_or("systemctl not available")?;
        if !out.status.success() {
            return Err(failure(&out, "systemctl start failed"));
        }
    } else {
        let binary = daemon_binary().ok_or("uscreen daemon binary not found. Run ./build-all.sh first.")?;
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

#[tauri::command]
pub async fn stop_daemon() -> Result<String, String> {
    if !pid_alive().await && !unit_active().await {
        return Ok("Daemon is not running".into());
    }
    if unit_active().await {
        let out = systemctl(&["stop", UNIT]).await.ok_or("systemctl not available")?;
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

#[tauri::command]
pub async fn get_autostart_status() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-enabled", "uscreen.service"])
        .output()
        .await
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled")
}

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
