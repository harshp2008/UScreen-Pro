//! Android 11+ wireless-debugging pairing: QR flow (mDNS) and manual `IP:port + code` fallback.
//!
//! QR flow: the phone scans `WIFI:T:ADB;S:<name>;P:<code>;;` and starts advertising
//! `_adb-tls-pairing._tcp` under instance name `<name>`. We watch `adb mdns services` for that
//! exact instance, run `adb pair <addr> <code>`, then look up the *separate* connect port
//! (`_adb-tls-connect._tcp`, same IP) and `adb connect` to it.

use crate::adb::{self, combined_of, stdout_of};
use crate::config;
use rand::Rng;
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const PAIRING_TYPE: &str = "_adb-tls-pairing._tcp";
const CONNECT_TYPE: &str = "_adb-tls-connect._tcp";
/// Hard limit on the `adb mdns services` discovery loop. Many routers drop multicast, so
/// waiting longer than this only leaves the user staring at a spinner.
const QR_TIMEOUT: Duration = Duration::from_secs(30);
/// Event the frontend listens for when discovery gave up.
/// Event carrying the current pairing step as plain text.
pub const PAIR_PROGRESS: &str = "pair_progress";
pub const MDNS_TIMEOUT_EVENT: &str = "mdns_timeout";
const MDNS_TIMEOUT_MESSAGE: &str =
    "mDNS discovery timed out. Your Wi-Fi router may be blocking device discovery. Please use Manual Pairing.";

#[derive(Serialize, Clone, Debug)]
pub struct QrPairingSession {
    pub service_name: String,
    pub pairing_code: String,
    pub qr_payload: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct PairingEvent {
    /// "searching" | "stalled" | "pairing" | "success" | "error" | "timeout"
    pub kind: &'static str,
    pub message: String,
    /// Set on "success" when `adb connect` succeeded.
    pub connect_endpoint: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ManualPairResult {
    pub message: String,
    /// None when the connect port could not be discovered via mDNS; the UI then asks for it.
    pub connect_endpoint: Option<String>,
}

/// Generation counter: starting or cancelling a session bumps it, which stops older poll loops.
#[derive(Default)]
pub struct PairingState(pub Arc<AtomicU64>);

struct MdnsService {
    name: String,
    kind: String,
    addr: String,
}

impl MdnsService {
    fn ip(&self) -> &str {
        self.addr.rsplit_once(':').map_or(&self.addr, |(ip, _)| ip)
    }
}

async fn mdns_services() -> Vec<MdnsService> {
    let Ok(out) = adb::run(None, &["mdns", "services"], Duration::from_secs(5)).await else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            // <instance>\t<type>.\t<ip:port>
            let mut parts = line.split_whitespace();
            let (name, kind, addr) = (parts.next()?, parts.next()?, parts.next()?);
            addr.contains(':').then(|| MdnsService {
                name: name.to_string(),
                kind: kind.trim_end_matches('.').to_string(),
                addr: addr.to_string(),
            })
        })
        .collect()
}

async fn discover_connect_endpoint(ip: &str, wait: Duration) -> Option<String> {
    let start = Instant::now();
    loop {
        let found = mdns_services()
            .await
            .into_iter()
            .find(|s| s.kind == CONNECT_TYPE && s.ip() == ip);
        if let Some(s) = found {
            return Some(s.addr);
        }
        if start.elapsed() >= wait {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}

async fn adb_pair(endpoint: &str, code: &str) -> Result<String, String> {
    let out = adb::run(None, &["pair", endpoint, code], Duration::from_secs(20)).await?;
    // `adb pair` exits 0 even on "Failed: Wrong password", so the output is the only signal.
    let text = combined_of(&out);
    if text.contains("Successfully paired") {
        Ok(stdout_of(&out))
    } else {
        Err(if text.is_empty() { "adb pair returned no output".into() } else { text })
    }
}

async fn adb_connect(endpoint: &str) -> Result<String, String> {
    let out = adb::run(None, &["connect", endpoint], Duration::from_secs(15)).await?;
    let text = combined_of(&out);
    if text.contains("connected to") || text.contains("already connected") {
        Ok(endpoint.to_string())
    } else {
        Err(if text.is_empty() { format!("Could not connect to {endpoint}") } else { text })
    }
}

fn emit(app: &AppHandle, kind: &'static str, message: impl Into<String>, connect: Option<String>) {
    let _ = app.emit(
        "pairing_event",
        PairingEvent { kind, message: message.into(), connect_endpoint: connect },
    );
}

#[tauri::command]
pub async fn initiate_qr_pairing(
    app: AppHandle,
    state: State<'_, PairingState>,
) -> Result<QrPairingSession, String> {
    let (service_name, pairing_code) = {
        let mut rng = rand::thread_rng();
        (
            format!("UScreen-{:04X}", rng.gen_range(0x1000u32..=0xFFFF)),
            rng.gen_range(100_000u32..1_000_000).to_string(),
        )
    };
    let qr_payload = format!("WIFI:T:ADB;S:{service_name};P:{pairing_code};;");

    let generation = state.0.fetch_add(1, Ordering::SeqCst) + 1;
    let counter = state.0.clone();
    let (name, code) = (service_name.clone(), pairing_code.clone());

    tokio::spawn(async move {
        let alive = || counter.load(Ordering::SeqCst) == generation;
        let start = Instant::now();
        let mut last_attempt: Option<String> = None;

        adb::progress(Some(&app), PAIR_PROGRESS, None, "Starting ADB server");
        let _ = adb::run(None, &["start-server"], Duration::from_secs(10)).await;
        emit(&app, "searching", "Waiting for the tablet to scan the QR code", None);
        while alive() && start.elapsed() < QR_TIMEOUT {
            // `adb mdns services` can itself hang for its full 5 s timeout, so cap each poll
            // at whatever is left of the 30 s budget.
            let remaining = QR_TIMEOUT.saturating_sub(start.elapsed());
            let hit = tokio::time::timeout(remaining, mdns_services())
                .await
                .unwrap_or_default()
                .into_iter()
                .find(|s| s.kind == PAIRING_TYPE && s.name == name);

            if let Some(svc) = hit
                && last_attempt.as_deref() != Some(svc.addr.as_str())
            {
                last_attempt = Some(svc.addr.clone());
                adb::progress(Some(&app), PAIR_PROGRESS, None, "Pairing...");
                emit(&app, "pairing", format!("Tablet found at {}, pairing", svc.addr), None);
                match adb_pair(&svc.addr, &code).await {
                    Ok(_) => {
                        adb::progress(Some(&app), PAIR_PROGRESS, None, "Connecting...");
                        let connect = match discover_connect_endpoint(svc.ip(), Duration::from_secs(10)).await {
                            Some(ep) => adb_connect(&ep).await.ok(),
                            None => None,
                        };
                        if alive() {
                            emit(&app, "success", format!("Paired with {}", svc.addr), connect);
                        }
                        counter.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    Err(e) => emit(&app, "error", e, None),
                }
            }

            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        if alive() {
            let _ = app.emit(MDNS_TIMEOUT_EVENT, MDNS_TIMEOUT_MESSAGE);
            emit(&app, "timeout", MDNS_TIMEOUT_MESSAGE, None);
        }
    });

    Ok(QrPairingSession { service_name, pairing_code, qr_payload })
}

#[tauri::command]
pub fn cancel_qr_pairing(state: State<'_, PairingState>) {
    state.0.fetch_add(1, Ordering::SeqCst);
}

/// Manual fallback: `pair_endpoint` and `code` come from the tablet's "Pair with pairing code"
/// dialog; `connect_endpoint` is the IP:port on the main Wireless debugging screen (optional,
/// discovered via mDNS when omitted).
#[tauri::command]
pub async fn manual_adb_pair(
    app: AppHandle,
    pair_endpoint: String,
    code: String,
    connect_endpoint: Option<String>,
) -> Result<ManualPairResult, String> {
    let pair_endpoint = pair_endpoint.trim().to_string();
    adb::progress(Some(&app), PAIR_PROGRESS, None, "Starting ADB server");
    let _ = adb::run(None, &["start-server"], Duration::from_secs(10)).await;
    adb::progress(Some(&app), PAIR_PROGRESS, None, "Pairing...");
    let message = adb_pair(&pair_endpoint, code.trim()).await?;

    let target = match connect_endpoint.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()) {
        Some(ep) => Some(ep),
        None => {
            let ip = pair_endpoint.rsplit_once(':').map_or(pair_endpoint.as_str(), |(ip, _)| ip);
            discover_connect_endpoint(ip, Duration::from_secs(6)).await
        }
    };
    let connect_endpoint = match target {
        Some(ep) => {
            adb::progress(Some(&app), PAIR_PROGRESS, None, "Connecting...");
            Some(adb_connect(&ep).await?)
        }
        None => None,
    };
    Ok(ManualPairResult { message, connect_endpoint })
}

#[tauri::command]
pub async fn manual_adb_connect(endpoint: String) -> Result<String, String> {
    adb_connect(endpoint.trim()).await
}

/// IPv4 address of the device's Wi-Fi interface, read over its USB shell.
async fn device_wlan_ip(serial: &str) -> Option<String> {
    let out = adb::run(Some(serial), &["shell", "ip", "-f", "inet", "addr", "show", "wlan0"], Duration::from_secs(5))
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        let l = l.trim();
        let cidr = l.strip_prefix("inet ")?.split_whitespace().next()?;
        Some(cidr.split_once('/').map_or(cidr, |(ip, _)| ip).to_string())
    })
}

/// Wireless setup runs one device at a time. Two concurrent runs would both restart adbd / poke the
/// shared adb server's mDNS and connect state, and the second one fails intermittently.
static WIRELESS_SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Android API level of the device (`ro.build.version.sdk`), read over its own USB shell.
async fn device_sdk(serial: &str) -> Option<u32> {
    let out = adb::run(Some(serial), &["shell", "getprop", "ro.build.version.sdk"], Duration::from_secs(5))
        .await
        .ok()?;
    stdout_of(&out).trim().parse().ok()
}

/// Can this computer reach `ip` over IP at all? A closed port answers with a reset (reachable);
/// only a silent timeout means the router/AP is dropping the traffic (client isolation, guest
/// network, MAC filtering) and no amount of adb retrying will help.
async fn host_reachable(ip: &str) -> bool {
    use tokio::net::TcpStream;
    for port in [5555u16, 5037, 80] {
        match tokio::time::timeout(Duration::from_millis(1500), TcpStream::connect((ip, port))).await {
            Ok(Ok(_)) => return true, // connected
            // A reset means the host itself answered. Any other error (EHOSTUNREACH after a failed
            // ARP, ENETUNREACH) or a timeout means it did not.
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => return true,
            Ok(Err(_)) | Err(_) => continue,
        }
    }
    false
}

/// Error prefix of a failed reachability check, parsed by the wizard's router-block banner.
pub const ROUTER_BLOCKED_PREFIX: &str = "ROUTER_BLOCKED|";
/// adb's classic TCP port. Tailscale has no mDNS, so the Tailscale path always uses it.
const TAILSCALE_ADB_PORT: u16 = 5555;

fn parse_ipv4(label: &str, value: &str) -> Result<std::net::Ipv4Addr, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("{label} \"{}\" is not a valid IPv4 address (Tailscale addresses look like 100.x.y.z).", value.trim()))
}

/// IPv4 address Tailscale assigned to this computer (`tailscale ip -4`), for auto-filling the wizard.
#[tauri::command]
pub async fn detect_tailscale_ip() -> Result<String, String> {
    let out = tokio::time::timeout(
        Duration::from_secs(4),
        tokio::process::Command::new("tailscale").args(["ip", "-4"]).kill_on_drop(true).output(),
    )
    .await
    .map_err(|_| "tailscale ip -4 timed out".to_string())?
    .map_err(|e| format!("Tailscale is not installed or not on PATH: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .map(str::trim)
        .find(|l| l.parse::<std::net::Ipv4Addr>().is_ok())
        .map(str::to_string)
        .ok_or_else(|| "Tailscale is installed but has no IPv4 address. Is it logged in and running?".to_string())
}

/// Tailscale path: connect to `target_ip:5555` through the tunnel and remember both addresses.
///
/// There is no mDNS across a tailnet, so this uses classic adb-over-TCP on 5555. When `serial` is given
/// (the device is on the USB cable right now) adbd is first switched to TCP mode with `adb tcpip 5555`,
/// which is what makes that port exist on the device. On success the `ip:port` is returned and both
/// addresses are saved to config.toml as `tailscale_host_ip` / `tailscale_target_ip`.
#[tauri::command]
pub async fn pair_via_tailscale(host_ip: String, target_ip: String, serial: Option<String>) -> Result<String, String> {
    let host = parse_ipv4("The Tailscale IP of this PC", &host_ip)?;
    let target = parse_ipv4("The Tailscale IP of the target device", &target_ip)?;
    let _one_at_a_time = WIRELESS_SETUP.lock().await;

    let _ = adb::run(None, &["start-server"], Duration::from_secs(10)).await;
    if let Some(serial) = serial.as_deref().filter(|s| !s.is_empty()) {
        adb::require_serial(serial)?;
        // Best effort: the user may also have enabled TCP mode some other way.
        let _ = adb::run(Some(serial), &["tcpip", &TAILSCALE_ADB_PORT.to_string()], Duration::from_secs(10)).await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }

    let endpoint = format!("{target}:{TAILSCALE_ADB_PORT}");
    let mut last = String::new();
    let mut connected = false;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(900)).await;
        }
        match adb_connect(&endpoint).await {
            Ok(_) => {
                connected = true;
                break;
            }
            Err(e) => last = e,
        }
    }
    if !connected {
        return Err(format!(
            "Could not connect via Tailscale. Please ensure: 1. Tailscale is running on both devices. \
             2. Both devices are logged into the same account (Tailnet). 3. The IP addresses entered are correct.\n\n\
             Details: {last}"
        ));
    }

    let mut cfg = config::load().await;
    cfg.extra.insert("tailscale_host_ip".into(), toml::Value::String(host.to_string()));
    cfg.extra.insert("tailscale_target_ip".into(), toml::Value::String(target.to_string()));
    config::store(&cfg).await?;
    Ok(endpoint)
}

/// "Setup via USB": switch the plugged-in device to wireless debugging without any pairing
/// code. Over USB the host already has an authorized adb shell, so it can enable Wireless
/// debugging itself (`settings put global adb_wifi_enabled 1`), read the Wi-Fi IP, and connect
/// to the TLS port advertised via mDNS. If mDNS is blocked it falls back to `adb tcpip 5555`,
/// which needs no discovery at all. Returns the `ip:port` that `adb connect` accepted.
#[tauri::command]
pub async fn bootstrap_wireless_over_usb(app: AppHandle, serial: String) -> Result<String, String> {
    adb::require_serial(&serial)?;
    let _one_at_a_time = WIRELESS_SETUP.lock().await;
    let app = Some(&app);
    adb::progress(app, PAIR_PROGRESS, Some(&serial), "Starting ADB server");
    let _ = adb::run(None, &["start-server"], Duration::from_secs(10)).await;
    adb::progress(app, PAIR_PROGRESS, Some(&serial), "Pairing...");
    let ip = device_wlan_ip(&serial)
        .await
        .ok_or("The device has no Wi-Fi address. Connect it to the same Wi-Fi network as this computer.")?;

    // Fail fast, and say why, when the network path is dead: otherwise this spins for ~90 s and ends
    // in a vague "adb connect timed out".
    if !host_reachable(&ip).await {
        // The `ROUTER_BLOCKED|<ip>|` prefix lets the wizard show its dedicated "Connection Blocked by
        // Router" banner; the rest is a readable message for any other caller.
        return Err(format!(
            "{ROUTER_BLOCKED_PREFIX}{ip}|This computer can see the device over USB but can't reach it at {ip} over Wi-Fi. \
             The router is dropping traffic between them (MAC randomization, AP/client isolation or a guest network)."
        ));
    }

    // Preferred: Android 11+ Wireless debugging (TLS), port found through mDNS. Older devices have no
    // such feature, so they go straight to `adb tcpip`.
    let sdk = device_sdk(&serial).await;
    if sdk.is_none_or(|v| v >= 30) {
        let _ = adb::run(Some(&serial), &["shell", "settings", "put", "global", "adb_wifi_enabled", "1"], Duration::from_secs(5)).await;
        if let Some(endpoint) = discover_connect_endpoint(&ip, Duration::from_secs(8)).await {
            adb::progress(app, PAIR_PROGRESS, Some(&serial), "Connecting...");
            if let Ok(ep) = adb_connect(&endpoint).await {
                return Ok(ep);
            }
        }
    }

    // Fallback: classic adb-over-TCP, restarted by adbd on port 5555.
    let out = adb::run(Some(&serial), &["tcpip", "5555"], Duration::from_secs(10)).await?;
    let text = combined_of(&out);
    if !out.status.success() && !text.contains("restarting in TCP mode") {
        return Err(format!("adb tcpip failed: {text}"));
    }
    let endpoint = format!("{ip}:5555");
    adb::progress(app, PAIR_PROGRESS, Some(&serial), "Connecting...");
    let mut last = String::new();
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(900)).await;
        match adb_connect(&endpoint).await {
            Ok(ep) => return Ok(ep),
            Err(e) => last = e,
        }
    }
    Err(format!("Could not connect to {endpoint}: {last}"))
}

#[cfg(test)]
mod reachability_tests {
    use super::host_reachable;

    #[tokio::test]
    async fn refused_port_on_a_live_host_counts_as_reachable() {
        assert!(host_reachable("127.0.0.1").await);
    }

    #[tokio::test]
    async fn blackholed_address_is_unreachable() {
        // TEST-NET-1 (RFC 5737) is never routed: a timeout or "unreachable" error, never a reset.
        assert!(!host_reachable("192.0.2.1").await);
    }
}
