//! ADB device tracking, transport negotiation, and tunnel management.
//!
//! This module provides asynchronous primitives for detecting Android devices
//! connected via USB or wireless networking (TCP/IP and Android 11+ TLS mDNS pairing),
//! negotiating reverse port tunnels for low-latency video and input streaming,
//! and supervising client APK launch state.

use anyhow::Result;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::config;
use crate::pairing;

/// Physical or wireless transport mechanism through which an ADB target is reached.
///
/// While the application-layer streaming protocol operates identically over any
/// stream-oriented transport, physical USB connections typically exhibit single-digit
/// millisecond frame jitter, whereas Wi-Fi links may suffer from 802.11 contention
/// and packet buffering. Distinguishing transport type allows intelligent prioritization
/// and user-facing diagnostic telemetry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transport {
    /// Direct physical USB connection via ADB USB multiplexer.
    Usb,
    /// Wireless connection via TCP/IP socket or Android 11+ mDNS TLS service.
    Network,
}

impl Transport {
    /// Return a human-readable display label for the transport type.
    pub fn label(self) -> &'static str {
        match self {
            Transport::Usb => "USB",
            Transport::Network => "Wi-Fi",
        }
    }
}

/// Determine whether a device serial represents a network endpoint rather than a physical USB ID.
///
/// Under ADB conventions, network endpoints are represented either as `ip:port` (legacy TCP/IP)
/// or as an mDNS identifier (such as `adb-R52WA0A0KED-aBcDeF._adb-tls-connect._tcp`). Physical
/// USB serials contain neither colons nor mDNS suffix patterns.
pub fn is_network_serial(serial: &str) -> bool {
    serial.contains(':') || serial.contains("._adb")
}

/// Derive the transport classification of an ADB serial identifier.
pub fn transport_of(serial: &str) -> Transport {
    if is_network_serial(serial) {
        Transport::Network
    } else {
        Transport::Usb
    }
}

/// Check whether the specified serial is an artificial test device configured via `USCREEN_FAKE_TABLET`.
///
/// This environment hook allows end-to-end multi-device testing and GUI development without requiring
/// physical Android hardware attached to the host workstation.
pub fn is_fake_serial(serial: &str) -> bool {
    std::env::var("USCREEN_FAKE_TABLET")
        .map(|f| f.split(',').any(|x| x.trim() == serial))
        .unwrap_or(false)
}

/// Metadata describing a discovered Android target device enumerated by ADB.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredDevice {
    /// The unique ADB identifier (USB serial number or network address:port).
    pub serial: String,
    /// The ADB connection state (e.g., "device", "offline", "unauthorized").
    pub state: String,
    /// Derived transport medium (USB vs. Network).
    pub transport: Transport,
    /// Resolved persistent physical hardware ID (used for seamless multi-transport handoff).
    pub hw_id: String,
}

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

/// Return the serial of the primary attached device, preferring physical USB over Wi-Fi.
#[allow(dead_code)]
pub async fn adb_device_serial() -> Option<String> {
    let out = Command::new("adb").arg("devices").output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let ready: Vec<&str> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            (state == "device").then_some(serial)
        })
        .collect();

    ready
        .iter()
        .find(|s| transport_of(s) == Transport::Usb)
        .or_else(|| ready.first())
        .map(|s| s.to_string())
}

/// Select which attached device to drive, retaining any currently active device if still valid.
#[allow(dead_code)]
pub async fn pick_device(devices: &[String], current: Option<&str>) -> Option<String> {
    if let Some(cur) = current {
        if devices.iter().any(|d| d == cur) {
            return Some(cur.to_string());
        }
    }
    let usb: Vec<&String> = devices
        .iter()
        .filter(|s| transport_of(s) == Transport::Usb)
        .collect();

    for d in &usb {
        if let Some(true) = app_running(d).await {
            return Some((*d).clone());
        }
    }
    if let Some(first_usb) = usb.first() {
        return Some((*first_usb).clone());
    }
    devices.first().cloned()
}

/// Establish reverse port forwarding tunnels via ADB to route client TCP connections back to host ports.
///
/// Retries up to 3 times with exponential backoff to handle transient ADB daemon restarts.
/// For slot 0 (video port 8890), fallback legacy ports (8890/8891) are mapped to maintain backwards
/// compatibility with legacy APK builds.
pub async fn setup_adb_forwarding(serial: &str, video_port: u16, input_port: u16) -> Result<()> {
    info!(
        "[ADB-FORWARD] Setting up reverse port forwarding for {} (video={}, input={})",
        serial, video_port, input_port
    );
    for port in [video_port, input_port] {
        let arg = format!("tcp:{}", port);
        let mut last_err = String::new();
        let mut success = false;
        for attempt in 1..=3 {
            let r = tokio::time::timeout(
                Duration::from_secs(2),
                Command::new("adb")
                    .args(["-s", serial, "reverse", &arg, &arg])
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
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
                Ok(Err(e)) => {
                    last_err = e.to_string();
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
                Err(_) => {
                    last_err = format!("adb reverse {} timed out", arg);
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        }
        if !success {
            anyhow::bail!("adb reverse {} failed after 3 attempts: {}", arg, last_err);
        }
    }
    if video_port == 8890 {
        let _ = Command::new("adb")
            .args(["-s", serial, "reverse", "tcp:8890", "tcp:8890"])
            .output()
            .await;
        let _ = Command::new("adb")
            .args(["-s", serial, "reverse", "tcp:8891", "tcp:8891"])
            .output()
            .await;
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
    let mut ports_to_remove = vec![video_port, input_port];
    if video_port == 8890 {
        ports_to_remove.push(8890);
        ports_to_remove.push(8891);
    }
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

/// Configure or teardown Wi-Fi TCP/IP debugging on the connected USB target.
pub async fn setup_wifi(off: bool) -> Result<()> {
    let mut cfg = config::FileConfig::load();

    if off {
        if !cfg.wifi_address.is_empty() {
            let _ = Command::new("adb")
                .args(["disconnect", &cfg.wifi_address])
                .output()
                .await;
        }
        cfg.wifi_address = String::new();
        cfg.save()?;
        println!("Wi-Fi off. Plug the cable in to use the device again.");
        return Ok(());
    }

    let Some(serial) = adb_devices()
        .await
        .into_iter()
        .find(|s| transport_of(s) == Transport::Usb)
    else {
        anyhow::bail!(
            "No device on USB. Plug the cable in for this one step — the device has to be told \
             to listen on the network, and only the cable can tell it."
        );
    };

    println!("Switching {} to Wi-Fi…", serial);
    let out = Command::new("adb")
        .args(["-s", &serial, "tcpip", "5555"])
        .output()
        .await?;
    if !out.status.success() {
        anyhow::bail!(
            "adb tcpip failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let Some(ip) = tablet_ip(&serial).await else {
        anyhow::bail!(
            "The device is listening, but its address could not be read. Find it under \
             Settings → About device → Status, then put `wifi_address = \"<ip>:5555\"` in \
             ~/.config/uscreen/config.toml."
        );
    };
    let address = format!("{}:5555", ip);

    let out = Command::new("adb")
        .args(["connect", &address])
        .output()
        .await?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !said.contains("connected") {
        anyhow::bail!("adb connect {} did not take: {}", address, said);
    }

    cfg.wifi_address = address.clone();
    cfg.save()?;
    println!("Connected to {}. The cable can come out.", address);
    println!(
        "The daemon reconnects to this address by itself whenever the cable is not in, \
         so this is a one-off — until the device reboots, which puts its adb back on USB \
         and means running this once more.\n\
         Wi-Fi is a fallback: the median latency matches the cable but single frames \
         arrive much later. `uscreen wifi --off` forgets the address."
    );
    Ok(())
}

/// Retrieve the active IPv4 address assigned to the tablet's wireless interface.
pub async fn tablet_ip(serial: &str) -> Option<String> {
    for args in [
        vec![
            "-s", serial, "shell", "ip", "-f", "inet", "addr", "show", "wlan0",
        ],
        vec!["-s", serial, "shell", "ip", "route", "get", "1.1.1.1"],
        vec!["-s", serial, "shell", "ip", "-f", "inet", "addr"],
    ] {
        let Ok(out) = Command::new("adb").args(&args).output().await else {
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let mut words = text.split_whitespace().peekable();
        while let Some(w) = words.next() {
            if w != "inet" && w != "src" {
                continue;
            }
            let Some(value) = words.peek() else { continue };
            let ip = value.split('/').next().unwrap_or(value);
            if ip.starts_with("127.") || !ip.contains('.') {
                continue;
            }
            if ip.split('.').count() == 4 && ip.split('.').all(|o| o.parse::<u8>().is_ok()) {
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// Automatically provision Wi-Fi ADB in the background when connected over USB.
///
/// On Android 11+ (API >= 30), provisions secure mDNS TLS debugging.
/// On Android < 11 (API < 30), respects the `allow_legacy_fallback` gate and dynamic port allocation.
pub async fn auto_provision_wifi(serial: &str) {
    if transport_of(serial) != Transport::Usb || is_fake_serial(serial) {
        return;
    }
    let s = serial.to_string();
    tokio::spawn(async move {
        let mut cfg = config::FileConfig::load();
        if let Err(e) = pairing::provision_wifi_for_device(
            &s,
            cfg.auto_enable_wifi_adb,
            cfg.allow_legacy_fallback,
            &mut cfg,
        )
        .await
        {
            warn!("Wi-Fi auto-provisioning failed for device {}: {}", s, e);
        }
    });
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_network_serials_from_usb_ones() {
        assert!(is_network_serial("192.168.1.20:5555"));
        assert!(is_network_serial(
            "adb-R52WA0A0KED-aBcDeF._adb-tls-connect._tcp"
        ));
        assert!(is_network_serial(
            "adb-R52WA0A0KED-aBcDeF._adb-tls-pairing._tcp"
        ));
        assert!(!is_network_serial("R52WA0A0KED"));
        assert!(!is_network_serial("emulator-5554"));
        assert_eq!(transport_of("R52WA0A0KED"), Transport::Usb);
        assert_eq!(
            transport_of("adb-X-y._adb-tls-connect._tcp"),
            Transport::Network
        );
    }

    #[test]
    fn physical_device_deduplication_prefers_usb() {
        let dev1 = DiscoveredDevice {
            serial: "192.168.1.102:5555".to_string(),
            state: "device".to_string(),
            transport: Transport::Network,
            hw_id: "PHONE_SERIAL".to_string(),
        };
        let dev2 = DiscoveredDevice {
            serial: "PHONE_SERIAL".to_string(),
            state: "device".to_string(),
            transport: Transport::Usb,
            hw_id: "PHONE_SERIAL".to_string(),
        };
        let dev3 = DiscoveredDevice {
            serial: "TABLET_SERIAL".to_string(),
            state: "device".to_string(),
            transport: Transport::Usb,
            hw_id: "TABLET_SERIAL".to_string(),
        };

        let mut map: HashMap<String, &DiscoveredDevice> = HashMap::new();
        for dev in &[&dev1, &dev2, &dev3] {
            map.entry(dev.hw_id.clone())
                .and_modify(|existing| {
                    if existing.transport != Transport::Usb && dev.transport == Transport::Usb {
                        *existing = dev;
                    }
                })
                .or_insert(dev);
        }

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("PHONE_SERIAL").unwrap().serial, "PHONE_SERIAL");
        assert_eq!(map.get("PHONE_SERIAL").unwrap().transport, Transport::Usb);
        assert_eq!(map.get("TABLET_SERIAL").unwrap().serial, "TABLET_SERIAL");
    }
}
