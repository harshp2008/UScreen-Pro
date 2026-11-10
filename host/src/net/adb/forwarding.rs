//! The `adb reverse` tunnels for video and input: which device ports map to which host ports for a session, and
//! setting them up and tearing them down. Owns the port mapping; discovery and launching the app are elsewhere.

use super::*;

/// The device-side ports the Android app falls back to when it is launched without port extras.
const APP_VIDEO_PORT: u16 = 8890;

/// The device-side input port the Android app falls back to when it is launched without port extras.
const APP_INPUT_PORT: u16 = 8891;

/// `(device port, host port)` pairs to reverse for one tablet.
///
/// The app is launched with the host's ports as extras, so each tablet's own ports are mapped
/// to themselves. The app's fixed defaults (8890/8891) are mapped to this tablet's host ports as
/// well: `adb reverse` is per device, so two tablets can both own device port 8890 while their
/// host listeners differ, and an app started by hand (without extras) still reaches its own
/// host session. Ported from upstream UScreen 1.2.14 ("fixed device ports").
fn reverse_pairs(video_port: u16, input_port: u16) -> Vec<(u16, u16)> {
    let mut pairs = vec![
        (video_port, video_port),
        (input_port, input_port),
        (APP_VIDEO_PORT, video_port),
        (APP_INPUT_PORT, input_port),
    ];
    let mut seen = Vec::new();
    pairs.retain(|p| {
        let new = !seen.contains(p);
        seen.push(*p);
        new
    });
    pairs
}

/// Establish reverse port forwarding tunnels via ADB to route client TCP connections back to host ports.
///
/// Retries up to 3 times with a short backoff to handle transient ADB daemon restarts. See
/// [`reverse_pairs`] for what is mapped.
pub async fn setup_adb_forwarding(serial: &str, video_port: u16, input_port: u16) -> Result<()> {
    info!(
        "[ADB-FORWARD] Setting up reverse port forwarding for {} (video={}, input={})",
        serial, video_port, input_port
    );
    for (device_port, host_port) in reverse_pairs(video_port, input_port) {
        let remote = format!("tcp:{}", device_port);
        let local = format!("tcp:{}", host_port);
        let mut last_err = String::new();
        let mut success = false;
        for attempt in 1..=3 {
            let r = tokio::time::timeout(
                Duration::from_secs(2),
                Command::new("adb")
                    .args(["-s", serial, "reverse", &remote, &local])
                    .output(),
            )
            .await;
            match r {
                Ok(Ok(output)) if output.status.success() => {
                    success = true;
                    break;
                }
                Ok(Ok(output)) => {
                    last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                }
                Ok(Err(e)) => {
                    last_err = e.to_string();
                }
                Err(_) => {
                    last_err = format!("adb reverse {} {} timed out", remote, local);
                }
            }
            if attempt < 3 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        if !success {
            anyhow::bail!(
                "adb reverse {} {} failed after 3 attempts: {}",
                remote,
                local,
                last_err
            );
        }
    }
    info!(
        "[ADB-FORWARD] Successfully established reverse port forwarding for {}",
        serial
    );
    Ok(())
}

/// Teardown reverse port forwarding tunnels for the given serial.
pub async fn teardown_adb_forwarding(serial: &str, video_port: u16, input_port: u16) {
    info!(
        "[ADB-FORWARD] Tearing down reverse port forwarding for {} (video={}, input={})",
        serial, video_port, input_port
    );
    let mut ports_to_remove: Vec<u16> = reverse_pairs(video_port, input_port)
        .into_iter()
        .map(|(device_port, _)| device_port)
        .collect();
    ports_to_remove.sort_unstable();
    ports_to_remove.dedup();
    for port in ports_to_remove {
        let arg = format!("tcp:{}", port);
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            Command::new("adb")
                .args(["-s", serial, "reverse", "--remove", &arg])
                .output(),
        )
        .await;
    }
}

/// Tests for the port mapping of `adb reverse`.
#[cfg(test)]
mod tests {
    use super::*;

    /// Reversed ports keep the app's fixed device ports for every host session: slot 0 maps each port once, later slots map their own ports and also the app's defaults.
    #[test]
    fn reverse_keeps_the_apps_fixed_device_ports_for_every_host_session() {
        // Slot 0 uses the defaults: nothing is mapped twice.
        assert_eq!(reverse_pairs(8890, 8891), vec![(8890, 8890), (8891, 8891)]);
        // Later slots keep their own ports and also expose the app's defaults.
        for (video, input) in [(8892, 8893), (8894, 8895), (9000, 9001)] {
            let pairs = reverse_pairs(video, input);
            assert!(pairs.contains(&(video, video)) && pairs.contains(&(input, input)));
            assert!(pairs.contains(&(8890, video)) && pairs.contains(&(8891, input)));
            assert_eq!(pairs.len(), 4);
        }
    }
}
