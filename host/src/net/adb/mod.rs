//! ADB device tracking, transport negotiation, and tunnel management.
//!
//! This module provides asynchronous primitives for detecting Android devices
//! connected via USB or wireless networking (TCP/IP and Android 11+ TLS mDNS pairing),
//! negotiating reverse port tunnels for low-latency video and input streaming,
//! and supervising client APK launch state.
//!
//! Layout: this file holds the shared types (`Transport`, `DiscoveredDevice`) and serial classification; the work is split into
//! `discovery` (finding devices), `forwarding` (`adb reverse` tunnels), `wifi` (wireless adb) and `app` (the tablet app).
//! Everything public is re-exported here, so callers keep using `crate::net::adb::name`.

use crate::config;
use crate::net::pairing;
use anyhow::Result;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{info, warn};

mod app;
mod discovery;
mod forwarding;
mod wifi;
pub use app::*;
pub use discovery::*;
pub use forwarding::*;
pub use wifi::*;

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

/// Tests for device-serial handling and USB-first deduplication.
#[cfg(test)]
mod tests {
    /// A device marked after an update is reopened once, and the mark goes when the app is launched for it.
    #[test]
    fn an_updated_device_is_marked_for_reopening() {
        assert!(!super::app::marked_for_reopen_for_test("unmarked"));
        super::reopen_after_update("marked-serial");
        assert!(super::app::marked_for_reopen_for_test("marked-serial"));
    }

    use super::app::ScreenState;

    /// `dumpsys window` excerpt with this app in front, as a Samsung launcher session reports it.
    const FOCUS_APP: &str = "  mCurrentFocus=Window{4f60b6d u0 com.uscreen/com.uscreen.MainActivity}\n    isKeyguardShowing=false\npackageName=com.sec.android.app.launcher\nmWakefulness=Awake\n";

    /// Our own app in front counts as idle; another app in front does not.
    #[test]
    fn screen_state_reads_focus_and_treats_only_idle_screens_as_idle() {
        let s = ScreenState::parse(FOCUS_APP);
        assert_eq!(s.focus.as_deref(), Some("com.uscreen"));
        assert_eq!(s.launcher.as_deref(), Some("com.sec.android.app.launcher"));
        assert!(s.is_idle());
        let chrome = FOCUS_APP.replace(
            "com.uscreen/com.uscreen.MainActivity",
            "com.android.chrome/org.chromium.Main",
        );
        assert!(!ScreenState::parse(&chrome).is_idle());
    }

    /// The home screen, the lock screen and a sleeping screen are idle.
    #[test]
    fn home_lock_and_sleep_are_idle() {
        let home = FOCUS_APP.replace(
            "com.uscreen/com.uscreen.MainActivity",
            "com.sec.android.app.launcher/x.Launcher",
        );
        assert!(ScreenState::parse(&home).is_idle());
        let chrome = FOCUS_APP.replace(
            "com.uscreen/com.uscreen.MainActivity",
            "com.android.chrome/y",
        );
        assert!(ScreenState::parse(
            &chrome.replace("isKeyguardShowing=false", "isKeyguardShowing=true")
        )
        .is_idle());
        assert!(
            ScreenState::parse(&chrome.replace("mWakefulness=Awake", "mWakefulness=Asleep"))
                .is_idle()
        );
        assert!(!ScreenState::parse("").is_idle());
    }

    use super::*;

    /// Network serials (`ip:port` and mDNS names) are told apart from USB serials.
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

    /// When one physical device shows up both by USB and over the network, the USB entry is kept.
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
