//! Checks of the connected tablet: adb, transport, app version

use super::*;

/// Doctor check of the tablet(s) visible over adb.
pub(super) async fn check_tablet(r: &mut Report, cfg: &FileConfig) {
    // `adb get-state` errors out when more than one device is attached, so
    // enumerate instead — that failure mode is silent otherwise.
    let Some(list) = output_of("adb", &["devices"]).await else {
        r.line(Level::Warn, "adb", "could not run");
        return;
    };
    let devices: Vec<&str> = list
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            (state == "device").then_some(serial)
        })
        .collect();

    // Which device every later check should address. With more than one
    // present, a bare `adb shell` fails outright ("more than one
    // device/emulator") — which is how this check used to report a perfectly
    // well installed app as missing the moment `adb tcpip` was in use.
    let chosen: Option<&str> = devices
        .iter()
        .find(|d| !crate::is_network_serial(d))
        .or_else(|| devices.first())
        .copied();

    match devices.len() {
        0 => {
            let unauthorized = list.contains("unauthorized");
            if unauthorized {
                r.line(Level::Fail, "tablet", "attached but unauthorized");
                r.hint("accept the 'Allow USB debugging' prompt on the tablet");
            } else {
                r.line(Level::Warn, "tablet", "not connected");
                r.hint("plug in the USB cable and enable USB debugging");
            }
            return;
        }
        1 => {
            r.line(Level::Ok, "tablet", devices[0]);
            report_transport(r, devices[0]);
        }
        n => {
            let pick = chosen.unwrap_or(devices[0]);
            // Two entries can mean one tablet reachable two ways — `adb tcpip`
            // leaves the cable working alongside the network device — or two
            // different Android devices plugged in, which is a different
            // situation entirely: the daemon drives the first and the app is
            // launched there, so watching the other one shows a black screen.
            // Only the second kind has more than one USB serial.
            let usb: Vec<&&str> = devices
                .iter()
                .filter(|d| !crate::is_network_serial(d))
                .collect();
            if usb.len() > 1 {
                let mut named = Vec::new();
                for d in &devices {
                    match output_of("adb", &["-s", d, "shell", "getprop", "ro.product.model"]).await
                    {
                        Some(m) if !m.trim().is_empty() => {
                            named.push(format!("{} ({})", d, m.trim()))
                        }
                        _ => named.push(d.to_string()),
                    }
                }
                r.line(
                    Level::Warn,
                    "tablet",
                    &format!("{} Android devices attached — using {}", n, pick),
                );
                r.hint(&format!(
                    "attached: {}. The daemon drives the first one and launches the app there, \
                     so a second device shows nothing. Unplug the others, or raise max_tablets \
                     to give each its own screen.",
                    named.join(", ")
                ));
            } else {
                r.line(
                    Level::Ok,
                    "tablet",
                    &format!("{} reachable {} ways: {:?}", pick, n, devices),
                );
            }
            report_transport(r, pick);
        }
    }
    let dev_args: Vec<String> = match chosen {
        Some(d) => vec!["-s".into(), d.to_string()],
        None => Vec::new(),
    };

    let mut reverse_args: Vec<&str> = dev_args.iter().map(|s| s.as_str()).collect();
    reverse_args.extend_from_slice(&["reverse", "--list"]);
    match output_of("adb", &reverse_args).await {
        Some(reverse) => {
            for port in [cfg.video_port, cfg.input_port] {
                let needle = format!("tcp:{}", port);
                if reverse.contains(&needle) {
                    r.line(Level::Ok, &format!("adb reverse {}", port), "forwarded");
                } else {
                    r.line(
                        Level::Warn,
                        &format!("adb reverse {}", port),
                        "not forwarded",
                    );
                    r.hint(&format!("adb reverse tcp:{p} tcp:{p}", p = port));
                }
            }
        }
        None => r.line(Level::Warn, "adb reverse", "could not query"),
    }

    // Whether this tablet can decode HEVC, and in 10 bits. Measured on one
    // Tab S9 Ultra, HEVC was slightly faster than H.264 and Main10 cost
    // nothing on top — but only where the hardware decoder exists, which is
    // exactly what this asks. H.264 stays the default because it is the one
    // every device has.
    let mut codec_args: Vec<&str> = dev_args.iter().map(|s| s.as_str()).collect();
    codec_args.extend_from_slice(&["shell", "dumpsys", "media.player"]);
    if let Some(out) = output_of("adb", &codec_args).await {
        let hevc = out.contains("video/hevc");
        let main10 = out.contains("Main10");
        match (hevc, main10, cfg.encoder.contains("hevc")) {
            (_, _, true) => r.line(
                Level::Ok,
                "tablet codec",
                "HEVC, and the host is sending it",
            ),
            (true, true, false) => {
                r.line(
                    Level::Ok,
                    "tablet codec",
                    "HEVC Main10 supported in hardware",
                );
                r.hint(
                    "optional: switch the encoder to hevc_nvenc for sharper text at the same \
                     bitrate, and tick 10-bit to smooth gradients",
                );
            }
            (true, false, false) => {
                r.line(Level::Ok, "tablet codec", "HEVC supported (8-bit)");
                r.hint("optional: switch the encoder to hevc_nvenc for sharper text");
            }
            (false, _, false) => r.line(
                Level::Ok,
                "tablet codec",
                "H.264 only — leave the encoder as it is",
            ),
        }
    }

    let mut pm_args: Vec<&str> = dev_args.iter().map(|s| s.as_str()).collect();
    pm_args.extend_from_slice(&["shell", "pm", "list", "packages", "com.uscreen"]);
    if let Some(out) = output_of("adb", &pm_args).await {
        if out.contains("com.uscreen") {
            r.line(Level::Ok, "tablet app", "installed");
        } else {
            r.line(Level::Fail, "tablet app", "com.uscreen not installed");
            r.hint("adb install android/app/build/outputs/apk/debug/app-debug.apk");
        }
    }
}

/// A network serial is `host:port`; a USB serial never contains a colon.
/// Worth reporting because the two transports differ by far more than the
/// median suggests — the Wi-Fi tail is several times worse.
pub(super) fn report_transport(r: &mut Report, serial: &str) {
    if crate::is_network_serial(serial) {
        r.line(
            Level::Warn,
            "transport",
            "Wi-Fi — expect occasional stutter",
        );
        r.hint("plug the USB cable in for steady latency; the daemon prefers it automatically");
    } else {
        r.line(Level::Ok, "transport", "USB");
    }
}
