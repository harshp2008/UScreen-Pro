//! Wireless adb: turning it on or off, finding the tablet's IP address and provisioning Wi-Fi automatically after a USB
//! connection. Owns the wireless setup; the tunnels and the app are elsewhere.
// PLATFORM(android): wireless debugging (adb pair / adb connect) exists on Android 11 and later

use super::*;

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
