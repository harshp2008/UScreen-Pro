//! Daemon runtime configuration synchronization, IPC control server, and PID lifecycle management.
//!
//! This module coordinates runtime persistence of user configurations, bidirectional IPC
//! with GUI / CLI clients over local Unix domain sockets, and PID file tracking to prevent
//! split-brain EVDI hardware collisions.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use crate::config::FileConfig;
use crate::session::DaemonControlMsg;

/// Synchronize the application window lock state with the GNOME Shell extension via JSON state.
///
/// When a tablet is mapped to a specific display connector, this function serializes the list
/// of pinned application window classes to `~/.local/share/uscreen/app_lock.json`. The companion
/// GNOME Shell extension observes this file and confines designated application windows to
/// the virtual display.
pub fn sync_app_lock(connector: Option<&str>, serial: Option<&str>) {
    let cfg = FileConfig::load();
    let apps = serial
        .and_then(|s| cfg.tablets.get(s))
        .and_then(|p| p.locked_apps.clone())
        .unwrap_or(cfg.locked_apps);

    let home = std::env::var("HOME").unwrap_or_default();
    let dir = PathBuf::from(home).join(".local/share/uscreen");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("app_lock.json");

    let enabled = !apps.is_empty() && connector.is_some();
    let json = serde_json::json!({
        "enabled": enabled,
        "connector": connector.unwrap_or(""),
        "locked_apps": apps,
    });
    if let Ok(content) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(path, content);
    }
}

/// Send an asynchronous single-line control command to a running uscreen daemon over the control socket.
///
/// Used by CLI subcommands (`uscreen pause-device <serial>`, `uscreen resume-device <serial>`)
/// to communicate directly with the active background daemon without restarting it.
pub async fn client_control_cmd(cmd: &str) -> Result<()> {
    let control_sock_path = crate::runtime::runtime_dir().join("control.sock");
    if !control_sock_path.exists() {
        anyhow::bail!(
            "uscreen daemon is not running (control socket {:?} not found)",
            control_sock_path
        );
    }
    let mut stream = UnixStream::connect(&control_sock_path).await?;
    stream.write_all(format!("{}\n", cmd).as_bytes()).await?;
    let mut reader = tokio::io::BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let resp = line.trim();
    if let Some(err) = resp.strip_prefix("ERR ") {
        anyhow::bail!("Control command failed: {}", err);
    }
    println!("{}", resp);
    Ok(())
}

/// Run the background Unix domain socket control server.
///
/// Accepts control commands from local clients (`PAUSE <serial>`, `RESUME <serial>`, `PING`)
/// and routes them to the session state machine over a bounded message channel.
pub async fn run_control_server(
    listener: UnixListener,
    control_tx: mpsc::Sender<DaemonControlMsg>,
) {
    loop {
        let (mut socket, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let ctrl_tx = control_tx.clone();
        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = tokio::io::BufReader::new(reader);
            let mut line = String::new();
            while let Ok(n) = buf_reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let cmd = line.trim();
                if let Some(serial) = cmd.strip_prefix("PAUSE ") {
                    let serial = serial.trim().to_string();
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::PauseDevice {
                            serial: serial.clone(),
                            resp: tx,
                        })
                        .await;
                    match rx.await {
                        Ok(Ok(())) => {
                            let _ = writer
                                .write_all(format!("OK PAUSED {}\n", serial).as_bytes())
                                .await;
                        }
                        Ok(Err(e)) => {
                            let _ = writer.write_all(format!("ERR {}\n", e).as_bytes()).await;
                        }
                        Err(_) => {
                            let _ = writer.write_all(b"ERR channel closed\n").await;
                        }
                    }
                } else if let Some(serial) = cmd.strip_prefix("RESUME ") {
                    let serial = serial.trim().to_string();
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::ResumeDevice {
                            serial: serial.clone(),
                            resp: tx,
                        })
                        .await;
                    match rx.await {
                        Ok(Ok(())) => {
                            let _ = writer
                                .write_all(format!("OK RESUMED {}\n", serial).as_bytes())
                                .await;
                        }
                        Ok(Err(e)) => {
                            let _ = writer.write_all(format!("ERR {}\n", e).as_bytes()).await;
                        }
                        Err(_) => {
                            let _ = writer.write_all(b"ERR channel closed\n").await;
                        }
                    }
                } else if let Some(serial) = cmd.strip_prefix("FREEZE ") {
                    let serial = serial.trim().to_string();
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::FreezeDevice { serial: serial.clone(), resp: tx })
                        .await;
                    let reply = match rx.await {
                        Ok(Ok(())) => format!("OK FROZEN {}\n", serial),
                        Ok(Err(e)) => format!("ERR {}\n", e),
                        Err(_) => "ERR channel closed\n".to_string(),
                    };
                    let _ = writer.write_all(reply.as_bytes()).await;
                } else if let Some(serial) = cmd.strip_prefix("THAW ") {
                    let serial = serial.trim().to_string();
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::ThawDevice { serial: serial.clone(), resp: tx })
                        .await;
                    let reply = match rx.await {
                        Ok(Ok(())) => format!("OK THAWED {}\n", serial),
                        Ok(Err(e)) => format!("ERR {}\n", e),
                        Err(_) => "ERR channel closed\n".to_string(),
                    };
                    let _ = writer.write_all(reply.as_bytes()).await;
                } else if cmd == "FROZEN" {
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx.send(DaemonControlMsg::ListFrozen { resp: tx }).await;
                    let list = rx.await.unwrap_or_default().join(",");
                    let _ = writer.write_all(format!("OK {}\n", list).as_bytes()).await;
                } else if cmd == "RESTART"
                    || cmd.starts_with("RESTART ")
                    || cmd == "RESTART_WAIT"
                    || cmd.starts_with("RESTART_WAIT ")
                {
                    let wait = cmd.starts_with("RESTART_WAIT");
                    let rest = cmd.strip_prefix("RESTART_WAIT").or_else(|| cmd.strip_prefix("RESTART")).unwrap_or("");
                    let serial = Some(rest.trim()).filter(|s| !s.is_empty() && *s != "ALL").map(str::to_string);
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::RestartSession { serial, resp: tx })
                        .await;
                    let reply = match rx.await {
                        Ok(Ok(slots)) => {
                            // The tablet's "Settings updating..." overlay stays up until the new
                            // encoder has delivered a frame (or 25 s pass), whoever is waiting.
                            let watchers: Vec<_> = slots
                                .iter()
                                .cloned()
                                .map(|slot| {
                                    tokio::spawn(async move {
                                        let live = crate::notices::wait_live(
                                            slot.instance,
                                            slot.generation_before,
                                            std::time::Duration::from_secs(25),
                                        )
                                        .await;
                                        crate::notices::release(slot.instance, crate::notices::settings_applied());
                                        (slot, live)
                                    })
                                })
                                .collect();
                            if wait {
                                let mut out = Vec::new();
                                for w in watchers {
                                    if let Ok((slot, live)) = w.await {
                                        out.push(serde_json::json!({
                                            "serial": slot.serial,
                                            "instance": slot.instance,
                                            "live_ms": live.map(|d| d.as_millis() as u64),
                                        }));
                                    }
                                }
                                format!("OK {}\n", serde_json::json!({ "restarted": out }))
                            } else {
                                format!("OK RESTARTED {}\n", slots.len())
                            }
                        }
                        Ok(Err(e)) => format!("ERR {}\n", e),
                        Err(_) => "ERR channel closed\n".to_string(),
                    };
                    let _ = writer.write_all(reply.as_bytes()).await;
                } else if let Some(serial) = cmd.strip_prefix("INSTALL ") {
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::InstallApk { serial: serial.trim().to_string(), resp: tx })
                        .await;
                    let reply = match rx.await {
                        Ok(Ok(msg)) => format!("OK {}\n", msg),
                        Ok(Err(e)) => format!("ERR {}\n", e),
                        Err(_) => "ERR channel closed\n".to_string(),
                    };
                    let _ = writer.write_all(reply.as_bytes()).await;
                } else if cmd == "INSTALLS" {
                    let _ = writer
                        .write_all(format!("OK {}\n", crate::installer::snapshot_json()).as_bytes())
                        .await;
                } else if let Some(serial) = cmd.strip_prefix("FORGET ") {
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx
                        .send(DaemonControlMsg::ForgetDevice { serial: serial.trim().to_string(), resp: tx })
                        .await;
                    let reply = match rx.await {
                        Ok(Ok(msg)) => format!("OK {}\n", msg),
                        Ok(Err(e)) => format!("ERR {}\n", e),
                        Err(_) => "ERR channel closed\n".to_string(),
                    };
                    let _ = writer.write_all(reply.as_bytes()).await;
                } else if let Some(arg) = cmd.strip_prefix("LATENCY") {
                    // Percentiles over the last N seconds (default 10), one JSON line.
                    let secs = arg.trim().parse::<u64>().unwrap_or(10).clamp(1, 60);
                    let _ = writer
                        .write_all(format!("{}\n", crate::latency::recent_json(secs)).as_bytes())
                        .await;
                } else if cmd == "PING" {
                    let _ = writer.write_all(b"PONG\n").await;
                } else {
                    let _ = writer.write_all(b"ERR unknown command\n").await;
                }
                let _ = writer.flush().await;
                line.clear();
            }
        });
    }
}

/// Remove the PID file only if its recorded process ID matches our own.
///
/// This check prevents race conditions where a freshly started daemon has already
/// overwritten the PID file during an overlapping restart cycle.
pub fn remove_pid_file_if_ours(pid_path: &Path, pid: u32) {
    let ours = std::fs::read_to_string(pid_path)
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
        .map(|p| p == pid)
        .unwrap_or(false);
    if ours {
        let _ = std::fs::remove_file(pid_path);
    }
}

/// Retrieve the canonical path of the daemon PID file (`~/.local/share/uscreen/uscreen.pid`).
pub fn get_pid_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{}/.local/share/uscreen/uscreen.pid", home))
}

/// Locate the `evdi_helper` binary across local development builds and installed system prefixes.
///
/// Evaluates explicit command line arguments, relative workspace paths, executable siblings,
/// and standard Linux distribution package directories (`/usr/lib/uscreen`, `/usr/libexec/uscreen`).
pub fn find_helper(path: &Path) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();

    if path.exists() {
        if let Ok(canon) = path.canonicalize() {
            return canon;
        }
        return path.to_path_buf();
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("evdi_helper"));
            if let Some(prefix) = dir.parent() {
                candidates.push(prefix.join("lib/uscreen/evdi_helper"));
                candidates.push(prefix.join("lib64/uscreen/evdi_helper"));
                candidates.push(prefix.join("libexec/uscreen/evdi_helper"));
            }
        }
    }
    candidates.extend(
        [
            format!("{}/.local/bin/evdi_helper", home),
            "/usr/lib/uscreen/evdi_helper".to_string(),
            "/usr/lib64/uscreen/evdi_helper".to_string(),
            "/usr/libexec/uscreen/evdi_helper".to_string(),
            "/usr/local/lib/uscreen/evdi_helper".to_string(),
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    for p in candidates {
        if p.exists() {
            return p;
        }
    }

    let alt = PathBuf::from("host/evdi/evdi_helper");
    if alt.exists() {
        if let Ok(canon) = alt.canonicalize() {
            return canon;
        }
        return alt;
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        for _ in 0..5 {
            if let Some(d) = dir {
                let from_exe = d.join("host").join("evdi").join("evdi_helper");
                if from_exe.exists() {
                    if let Ok(canon) = from_exe.canonicalize() {
                        return canon;
                    }
                    return from_exe;
                }
                dir = d.parent();
            } else {
                break;
            }
        }
    }

    path.to_path_buf()
}

/// Enumerate PIDs of any other running `uscreen` daemon processes owned by the current user.
///
/// Scans `/proc` to find instances of the `uscreen` binary running in daemon mode, avoiding
/// matching ephemeral commands such as `uscreen stop`, `uscreen status`, or shell wrappers.
pub fn other_daemons() -> Vec<u32> {
    use std::os::unix::fs::MetadataExt;
    let me = std::process::id();
    let ppid = unsafe { libc::getppid() as u32 };
    let uid = unsafe { libc::getuid() };
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for e in dir.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == me || pid == ppid {
            continue;
        }
        let base = e.path();
        let Ok(comm) = std::fs::read_to_string(base.join("comm")) else {
            continue;
        };
        if comm.trim() != "uscreen" {
            continue;
        }
        let Ok(cmd) = std::fs::read(base.join("cmdline")) else {
            continue;
        };
        let args: Vec<&[u8]> = cmd.split(|b| *b == 0).skip(1).collect();
        let other_subcommand = args.iter().any(|a| {
            matches!(
                *a,
                b"stop"
                    | b"status"
                    | b"list-displays"
                    | b"doctor"
                    | b"wifi"
                    | b"--help"
                    | b"-h"
                    | b"--version"
                    | b"-V"
                    | b"-d"
                    | b"--daemonize"
                    | b"pause-device"
                    | b"resume-device"
            )
        });
        if other_subcommand {
            continue;
        }
        if std::fs::metadata(&base)
            .map(|m| m.uid())
            .unwrap_or(u32::MAX)
            != uid
        {
            continue;
        }
        out.push(pid);
    }
    out
}
