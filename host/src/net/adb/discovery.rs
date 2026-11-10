//! Finding devices: the hardware-serial lookup, the `adb track-devices` watcher and the scan that merges USB and
//! network entries of one physical device (USB wins). Owns discovery only; tunnels and the app are elsewhere.

use super::*;

/// Resolve the underlying physical hardware serial number of an ADB endpoint.
///
/// When a tablet is connected simultaneously via USB and Wi-Fi, its network endpoint
/// displays an ephemeral IP:port or TLS pairing string. By querying `ro.serialno` (or checking
/// locally cached configuration mappings), this function resolves the invariant physical
/// hardware identifier, preventing duplicate session creation and enabling seamless
/// transport handoff without tearing down the virtual display.
pub async fn resolve_hw_id(serial: &str, cache: &mut HashMap<String, String>) -> String {
    if !is_network_serial(serial) {
        return serial.to_string();
    }
    if let Some(cached) = cache.get(serial) {
        return cached.clone();
    }
    let file_cfg = config::FileConfig::load();
    for (k, v) in &file_cfg.tablets {
        if v.wifi_address.as_deref() == Some(serial) {
            cache.insert(serial.to_string(), k.clone());
            return k.clone();
        }
    }
    if let Ok(Ok(output)) = tokio::time::timeout(
        Duration::from_millis(800),
        Command::new("adb")
            .args(["-s", serial, "shell", "getprop", "ro.serialno"])
            .output(),
    )
    .await
    {
        let hw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !hw.is_empty() && hw != "unknown" {
            cache.insert(serial.to_string(), hw.clone());
            return hw;
        }
    }
    serial.to_string()
}

/// Spawn an asynchronous background tracker that streams real-time ADB device events.
///
/// Executes `adb track-devices` in a non-blocking subprocess. Each emitted line signals
/// an attachment, detachment, or state transition, eliminating the latency and overhead
/// of polling `adb devices` in a tight loop.
pub fn spawn_adb_tracker() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        loop {
            let mut cmd = Command::new("adb");
            cmd.arg("track-devices")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());

            match cmd.spawn() {
                Ok(mut child) => {
                    if let Some(stdout) = child.stdout.take() {
                        use tokio::io::{AsyncBufReadExt, BufReader};
                        let mut reader = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            if tx.send(line).await.is_err() {
                                let _ = child.kill().await;
                                return;
                            }
                        }
                    }
                    let _ = child.wait().await;
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
    rx
}

/// Scan all attached ADB devices, verifying readiness and resolving physical hardware IDs.
///
/// In addition to parsing `adb devices -l`, this function addresses Android backgrounding
/// anomalies where devices intermittently report "unauthorized" during screen lock by
/// cross-verifying with `adb get-state`.
pub async fn scan_adb_devices(hw_cache: &mut HashMap<String, String>) -> Vec<DiscoveredDevice> {
    let Ok(out) = Command::new("adb").args(["devices", "-l"]).output().await else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut out_devices = Vec::new();

    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let Some(state_part) = parts.next() else {
            continue;
        };
        let mut state = state_part.to_string();
        if state == "unauthorized" {
            // Query adb -s <serial> get-state to avoid false unauthorized reports
            if let Ok(st_out) = Command::new("adb")
                .args(["-s", serial, "get-state"])
                .output()
                .await
            {
                let s = String::from_utf8_lossy(&st_out.stdout).trim().to_string();
                if s == "device" {
                    info!("Device {} reported 'unauthorized' by devices -l but get-state is 'device' (Waiting for app...)", serial);
                    state = "device".to_string();
                }
            }
        }
        let transport = transport_of(serial);
        let hw_id = if transport == Transport::Usb {
            serial.to_string()
        } else {
            resolve_hw_id(serial, hw_cache).await
        };
        out_devices.push(DiscoveredDevice {
            serial: serial.to_string(),
            state,
            transport,
            hw_id,
        });
    }

    // Support USCREEN_FAKE_TABLET test hook for headless/CI workflows
    if let Ok(fake) = std::env::var("USCREEN_FAKE_TABLET") {
        for f in fake.split(',').map(str::trim).filter(|f| !f.is_empty()) {
            if !out_devices.iter().any(|d| d.serial == f) {
                out_devices.push(DiscoveredDevice {
                    serial: f.to_string(),
                    state: "device".to_string(),
                    transport: transport_of(f),
                    hw_id: f.to_string(),
                });
            }
        }
    }

    out_devices
}

/// Retrieve a list of all currently authorized device serials, sorted with USB devices first.
pub async fn adb_devices() -> Vec<String> {
    let out = match Command::new("adb").args(["devices"]).output().await {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut ready: Vec<String> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            (state == "device").then(|| serial.to_string())
        })
        .collect();
    ready.sort_by_key(|s| transport_of(s) != Transport::Usb);
    ready
}
