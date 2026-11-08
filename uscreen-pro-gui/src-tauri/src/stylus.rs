//! Stylus profiles for the GUI. The profile files live in `~/.config/uscreen/stylus_profiles/` and are
//! read, validated and written by the `uscreen` daemon binary itself (`stylus-profiles`,
//! `stylus-profile-save`, `stylus-profile-delete`, `stylus-test`), so the GUI and the tablet app
//! share one parser and one list of events and can never disagree about what a mapping means.

use crate::daemon::daemon_binary;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

fn binary() -> Result<std::path::PathBuf, String> {
    daemon_binary()
        .ok_or_else(|| "uscreen daemon binary not found. Run ./build-all.sh first.".to_string())
}

/// Run the daemon binary with `args`, optionally feeding `stdin`; Ok is its stdout, Err its stderr.
async fn run(args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new(binary()?);
    cmd.args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(text.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        // The daemon logs through tracing too; the failure is the last "Error:" line.
        let msg = err
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix("Error: "))
            .unwrap_or(err.trim());
        Err(msg.to_string())
    }
}

/// Tauri command: every stylus profile and the events they can map (the same JSON the tablet gets).
#[tauri::command]
pub async fn list_stylus_profiles() -> Result<Value, String> {
    let text = run(&["stylus-profiles"], None).await?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Tauri command: create or replace a custom profile; returns its id. Invalid mappings are refused with the reason.
#[tauri::command]
pub async fn save_stylus_profile(profile: Value) -> Result<String, String> {
    run(&["stylus-profile-save"], Some(&profile.to_string())).await
}

/// Tauri command: delete a custom profile.
#[tauri::command]
pub async fn delete_stylus_profile(id: String) -> Result<(), String> {
    run(&["stylus-profile-delete", &id], None).await.map(|_| ())
}

/// Tauri command: run one action (`ctrl+c`, `mouse_middle`...) on this computer now, to try a mapping before saving it.
#[tauri::command]
pub async fn test_stylus_action(action: String) -> Result<String, String> {
    run(&["stylus-test", &action], None).await
}
