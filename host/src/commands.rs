//! One-shot subcommands that talk to an already-running daemon (`stop`, `status`) or inspect the
//! system (`list-displays`).

use anyhow::Result;
use tracing::info;

use crate::config_handler::{get_pid_path, other_daemons};

/// Send SIGTERM to any running uscreen daemon process owned by the current user.
pub(crate) async fn stop_daemon() -> Result<()> {
    let pid_path = get_pid_path();

    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path)?;
        let pid: u32 = pid_str.trim().parse()?;
        info!("Stopping uscreen daemon (PID: {})", pid);

        let result = tokio::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output()
            .await?;

        if result.status.success() {
            let _ = std::fs::remove_file(&pid_path);
            info!("uscreen daemon stopped");
        } else {
            for p in other_daemons() {
                unsafe {
                    libc::kill(p as i32, libc::SIGTERM);
                }
            }
            let _ = std::fs::remove_file(&pid_path);
            info!("uscreen daemon stopped (fallback)");
        }
    } else {
        for p in other_daemons() {
            unsafe {
                libc::kill(p as i32, libc::SIGTERM);
            }
        }
        info!("uscreen daemon stopped (no PID file)");
    }

    Ok(())
}

/// Inspect the filesystem and `/proc` to report daemon execution status and active PID.
pub(crate) async fn show_status() -> Result<()> {
    let pid_path = get_pid_path();
    let my_pid = std::process::id();

    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path)?;
        let pid: u32 = pid_str.trim().parse().unwrap_or(0);

        if pid > 0 && pid != my_pid {
            let proc_path = format!("/proc/{}", pid);
            if std::path::Path::new(&proc_path).exists() {
                println!(
                    "uscreen is running (PID: {}, version: {})",
                    pid,
                    env!("GIT_VERSION_STRING")
                );
            } else {
                println!("uscreen is not running (stale PID file)");
                let _ = std::fs::remove_file(&pid_path);
            }
        } else {
            println!(
                "uscreen is not running (version: {})",
                env!("GIT_VERSION_STRING")
            );
        }
    } else {
        let pids = other_daemons()
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if !pids.is_empty() {
            println!(
                "uscreen is running (PID: {}, version: {})",
                pids,
                env!("GIT_VERSION_STRING")
            );
        } else {
            println!(
                "uscreen is not running (version: {})",
                env!("GIT_VERSION_STRING")
            );
        }
    }
    Ok(())
}

/// Query desktop display subsystems (KScreen, PipeWire) to list detected video outputs.
pub(crate) async fn list_displays() -> Result<()> {
    println!("=== Available displays ===");
    if let Ok(out) = tokio::process::Command::new("kscreen-doctor")
        .args(["-o"])
        .output()
        .await
    {
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }

    if let Ok(out) = tokio::process::Command::new("wpctl")
        .args(["status"])
        .output()
        .await
    {
        println!("--- PipeWire ---");
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }
    Ok(())
}
