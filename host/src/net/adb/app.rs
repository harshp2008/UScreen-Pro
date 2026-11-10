//! The Android app on a connected tablet: whether it runs, launching it with the session's ports and token, and the
//! work done when a tablet first connects. Owns app supervision; discovery and tunnels are elsewhere.

use super::*;

/// Determine whether the UScreen client process (`com.uscreen`) is currently running on the device.
pub async fn app_running(serial: &str) -> Option<bool> {
    let out = Command::new("adb")
        .args(["-s", serial, "shell", "pidof", "com.uscreen"])
        .output()
        .await
        .ok()?;
    if !out.stderr.is_empty() {
        return None;
    }
    Some(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Emit diagnostic telemetry when a session is active over a Wi-Fi transport.
pub fn announce_transport(serial: &str) {
    if transport_of(serial) == Transport::Network {
        warn!("Running over Wi-Fi. Expect occasional stutter — the cable is much steadier.");
    }
}

/// Invoked when a tablet is attached or transitions to ready state.
///
/// Sets up port forwarding and optionally launches the client APK with intent parameters.
pub async fn on_tablet_connected(
    serial: &str,
    video_port: u16,
    input_port: u16,
    auto_launch: bool,
    token: Option<&str>,
) {
    match setup_adb_forwarding(serial, video_port, input_port).await {
        Ok(_) => {
            info!(
                "ADB port forwarding set up (video={}, input={})",
                video_port, input_port
            );
            if auto_launch {
                launch_app(serial, token, video_port, input_port).await;
            }
        }
        Err(e) => warn!("ADB forwarding failed: {}", e),
    }
}

/// Launch or front the UScreen Android client application, passing the security token and port parameters.
pub async fn launch_app(serial: &str, token: Option<&str>, video_port: u16, input_port: u16) {
    let mut cmd = format!(
        "am start -n com.uscreen/.MainActivity --ei video_port {} --ei input_port {}",
        video_port, input_port
    );
    if let Some(t) = token {
        cmd.push_str(" --es token ");
        cmd.push_str(t);
    }
    cmd.push_str(" >/dev/null 2>&1; exit\n");

    let child = Command::new("adb")
        .args(["-s", serial, "shell"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            warn!("Could not run adb: {}", e);
            return;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(cmd.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    match tokio::time::timeout(Duration::from_secs(15), child.wait()).await {
        Ok(Ok(st)) if st.success() => info!("UScreen app launched on device"),
        _ => warn!("Could not launch the app (is it installed?)"),
    }
}
