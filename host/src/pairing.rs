//! Host-side pairing and wireless-transport helpers built on adb: `adb pair` with a code, mDNS
//! discovery of the TLS connect endpoint, the legacy `adb tcpip` fallback for pre-Android-11
//! devices (with unique per-device ports), and Wi-Fi provisioning over an existing USB link.
//! The device's Android API level (see [`query_device_api_level`]) decides which path is used.

use anyhow::{Context, Result};
use tracing::{info, warn};

/// Queries the Android API level (SDK integer) of a connected device via adb shell.
pub async fn query_device_api_level(serial: &str) -> Option<u32> {
    let out = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell", "getprop", "ro.build.version.sdk"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim().parse::<u32>().ok()
}

/// Executes `adb pair <pairing_endpoint> <code>` for secure Android 11+ pairing.
#[allow(dead_code)]
pub async fn perform_secure_pair(pairing_endpoint: &str, code: &str) -> Result<String> {
    info!("Initiating secure TLS pairing with {}...", pairing_endpoint);
    let out = tokio::process::Command::new("adb")
        .args(["pair", pairing_endpoint, code])
        .output()
        .await
        .context("Failed to execute `adb pair`")?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    if out.status.success() || stdout.contains("Successfully paired") {
        info!("Successfully paired to {}: {}", pairing_endpoint, stdout);
        Ok(stdout)
    } else {
        let err_msg = if !stderr.is_empty() { stderr } else { stdout };
        anyhow::bail!("Pairing failed: {}", err_msg);
    }
}

/// Discovers the dynamic TLS connect endpoint (_adb-tls-connect._tcp) for a given IP
/// by polling `adb mdns services`.
pub async fn discover_mdns_tls_connect_endpoint(
    target_ip: &str,
    timeout_secs: u64,
) -> Option<String> {
    let start = std::time::Instant::now();
    let duration = std::time::Duration::from_secs(timeout_secs.max(1));

    while start.elapsed() < duration {
        if let Ok(out) = tokio::process::Command::new("adb")
            .args(["mdns", "services"])
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                // Lines are formatted as: <name>\t<service_type>\t<address:port>
                // e.g.: adb-TABLET_SERIAL-123456\t_adb-tls-connect._tcp.\t192.168.1.101:41235
                if line.contains("_adb-tls-connect._tcp") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for part in parts {
                        if let Some((ip, port_str)) = part.rsplit_once(':') {
                            if ip == target_ip {
                                if let Ok(port) = port_str.parse::<u16>() {
                                    info!(
                                        "mDNS discovered TLS connect endpoint for {}: {}:{}",
                                        target_ip, ip, port
                                    );
                                    return Some(format!("{}:{}", ip, port));
                                }
                            }
                        }
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    None
}

/// Allocates the next available collision-free legacy TCP port for Android <11.
/// Starts at 5556 to prevent collisions with the standard default 5555.
pub fn allocate_unique_legacy_port(existing_ports: &[u16]) -> u16 {
    let mut candidate = 5556u16;
    while candidate < 65535 {
        if candidate != 5555 && !existing_ports.contains(&candidate) {
            return candidate;
        }
        candidate = candidate.saturating_add(1);
    }
    5556
}

/// Configures legacy ADB over TCP/IP on a specific dynamic port.
pub async fn enable_legacy_tcpip(serial: &str, port: u16) -> Result<()> {
    info!("Setting legacy ADB tcpip port for {} to {}", serial, port);
    let out = tokio::process::Command::new("adb")
        .args(["-s", serial, "tcpip", &port.to_string()])
        .output()
        .await
        .context("Failed to execute `adb tcpip`")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Failed to set tcpip {}: {}", port, err);
    }
    Ok(())
}

/// Queries the IP of a connected Android device over USB adb shell.
pub async fn get_device_wlan_ip(serial: &str) -> Option<String> {
    let out = tokio::process::Command::new("adb")
        .args([
            "-s", serial, "shell", "ip", "-f", "inet", "addr", "show", "wlan0",
        ])
        .output()
        .await
        .ok()?;

    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("inet ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let cidr = parts[1];
                if let Some((ip, _)) = cidr.split_once('/') {
                    return Some(ip.to_string());
                }
                return Some(cidr.to_string());
            }
        }
    }
    None
}

/// Version-aware automatic provisioning pipeline called when a device connects via USB.
/// - If API >= 30: Enables wireless debugging if disabled, and attempts mDNS TLS discovery.
/// - If API < 30: Gates on allow_legacy_fallback and allocates a unique non-colliding port.
pub async fn provision_wifi_for_device(
    serial: &str,
    auto_enable_wifi_adb: bool,
    allow_legacy_fallback: bool,
    cfg: &mut crate::config::FileConfig,
) -> Result<Option<String>> {
    if !auto_enable_wifi_adb {
        info!(
            "Auto-enable Wi-Fi ADB is disabled in config. Skipping provisioning for {}",
            serial
        );
        return Ok(None);
    }

    let api_level = match query_device_api_level(serial).await {
        Some(lvl) => lvl,
        None => {
            warn!(
                "Could not determine Android API level for device {}",
                serial
            );
            return Ok(None);
        }
    };

    info!(
        "Device {} is running Android API level {}",
        serial, api_level
    );

    // Cache the API level in the profile
    let profile = cfg.tablets.entry(serial.to_string()).or_default();
    profile.api_level = Some(api_level);

    if api_level >= 30 {
        // Android 11+: Secure Pairing Architecture
        info!(
            "Device {} (API {}) supports Secure TLS Wireless Debugging",
            serial, api_level
        );

        // Ensure Wireless Debugging is enabled on the device
        let _ = tokio::process::Command::new("adb")
            .args([
                "-s",
                serial,
                "shell",
                "settings",
                "put",
                "global",
                "adb_wifi_enabled",
                "1",
            ])
            .output()
            .await;

        if let Some(ip) = get_device_wlan_ip(serial).await {
            info!("Device {} wlan0 IP: {}", serial, ip);
            // Check if mDNS already advertises the dynamic connect port
            if let Some(endpoint) = discover_mdns_tls_connect_endpoint(&ip, 3).await {
                info!(
                    "Device {} auto-provisioned via secure mDNS: {}",
                    serial, endpoint
                );
                if let Some((_, p_str)) = endpoint.rsplit_once(':') {
                    if let Ok(p) = p_str.parse::<u16>() {
                        let profile = cfg.tablets.entry(serial.to_string()).or_default();
                        profile.secure_port = Some(p);
                        profile.wifi_address = Some(endpoint.clone());
                        let _ = cfg.save();
                    }
                }
                let _ = tokio::process::Command::new("adb")
                    .args(["connect", &endpoint])
                    .output()
                    .await;
                return Ok(Some(endpoint));
            } else if let Some(ref remembered) = profile.wifi_address.clone() {
                // If not found immediately via mDNS, use remembered TLS address if matching IP
                if remembered.starts_with(&ip) {
                    let _ = tokio::process::Command::new("adb")
                        .args(["connect", remembered])
                        .output()
                        .await;
                    return Ok(Some(remembered.clone()));
                }
            }
        }
        Ok(None)
    } else {
        // Android <11: Legacy Fallback Architecture
        if !allow_legacy_fallback {
            warn!(
                "Device {} runs Android API {} (<30). 'allow_legacy_fallback' is false — Wi-Fi ADB provisioning blocked.",
                serial, api_level
            );
            return Ok(None);
        }

        info!(
            "Device {} runs Android API {} (<30). Provisioning via dynamic legacy port.",
            serial, api_level
        );

        // Collect existing assigned legacy ports to prevent port collision
        let mut existing_ports = Vec::new();
        for p in cfg.tablets.values() {
            if let Some(ref addr) = p.wifi_address {
                if let Some((_, port_str)) = addr.rsplit_once(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        existing_ports.push(port);
                    }
                }
            }
        }

        let dynamic_port = allocate_unique_legacy_port(&existing_ports);
        enable_legacy_tcpip(serial, dynamic_port).await?;

        if let Some(ip) = get_device_wlan_ip(serial).await {
            let endpoint = format!("{}:{}", ip, dynamic_port);
            info!(
                "Legacy device {} provisioned on unique port: {}",
                serial, endpoint
            );
            let profile = cfg.tablets.entry(serial.to_string()).or_default();
            profile.wifi_address = Some(endpoint.clone());
            let _ = cfg.save();

            let _ = tokio::process::Command::new("adb")
                .args(["connect", &endpoint])
                .output()
                .await;
            return Ok(Some(endpoint));
        }

        Ok(None)
    }
}
