//! Enumerating devices: parses `adb devices -l`, merges a tablet's USB and Wi-Fi entries into one logical device, and
//! reads the hardware serial and the installed app version. Owns the device list; installing and settings intents are elsewhere.

use super::*;

/// Parse `adb devices -l` into raw entries (one per transport). Returns an empty list if adb is missing or slow.
pub async fn list_entries() -> Vec<AdbEntry> {
    let Ok(out) = run(None, &["devices", "-l"], Duration::from_millis(2500)).await else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            let serial = parts[0].to_string();
            let model = parts[2..]
                .iter()
                .find_map(|p| p.strip_prefix("model:"))
                .map(|m| m.replace('_', " "))
                .unwrap_or_else(|| serial.clone());
            Some(AdbEntry {
                is_wifi: serial.contains(':') || serial.contains("._adb-tls-connect"),
                state: parts[1].to_string(),
                serial,
                model,
            })
        })
        .collect()
}

/// Tauri command: raw adb entries for the wizard's device picker.
#[tauri::command]
pub async fn list_adb_devices() -> Vec<AdbEntry> {
    list_entries().await
}

/// Normalises a model name for matching: trimmed, upper-case, dashes as spaces.
fn norm_model(m: &str) -> String {
    m.trim().to_uppercase().replace('-', " ")
}

/// The adb entries of one physical tablet, merged across USB and Wi-Fi: model, preferred serial, which transports exist, state and all serials.
struct Group {
    model: String,
    primary_serial: String,
    has_usb: bool,
    has_wifi: bool,
    state: String,
    all_serials: Vec<String>,
}

/// Normalised model label used to match the same tablet across USB and Wi-Fi entries.
pub(crate) fn norm_label(m: &str) -> String {
    norm_model(m)
}

/// Hardware serial (`ro.serialno`) of a device, which stays constant across transports unlike the adb serial.
pub(crate) async fn ro_serial(serial: &str) -> Option<String> {
    let out = run(
        Some(serial),
        &["shell", "getprop", "ro.serialno"],
        Duration::from_millis(3000),
    )
    .await
    .ok()?;
    let s = stdout_of(&out);
    (!s.is_empty() && s != "unknown").then_some(s)
}

/// The `versionName` of the Android app on the first of `serials` that reports one, or `None` when none does.
async fn installed_app_version(serials: &[String]) -> Option<String> {
    for s in serials {
        let Ok(out) = run(
            Some(s),
            &["shell", "dumpsys", "package", "com.uscreen"],
            Duration::from_millis(3000),
        )
        .await
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let version = text
            .lines()
            .find_map(|l| l.split("versionName=").nth(1))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if version.is_some() {
            return version;
        }
    }
    None
}

/// Merge a tablet's USB and Wi-Fi ADB entries into one device, then append saved-but-offline profiles.
pub async fn connected_devices(cfg: &AppConfig) -> Vec<ConnectedDevice> {
    let mut groups: Vec<Group> = Vec::new();

    for entry in list_entries().await {
        let ro = if entry.is_wifi && entry.state == "device" {
            ro_serial(&entry.serial).await
        } else {
            None
        };
        let norm = norm_model(&entry.model);
        let found = groups.iter().position(|g| {
            (!norm.is_empty() && norm == norm_model(&g.model))
                || ro
                    .as_ref()
                    .is_some_and(|r| g.primary_serial == *r || g.all_serials.contains(r))
                || (!entry.is_wifi && g.all_serials.contains(&entry.serial))
        });

        match found {
            Some(idx) => {
                let g = &mut groups[idx];
                g.all_serials.push(entry.serial.clone());
                if entry.is_wifi {
                    g.has_wifi = true;
                    if g.primary_serial.contains(':')
                        && let Some(r) = ro
                    {
                        g.primary_serial = r;
                    }
                } else {
                    g.has_usb = true;
                    g.primary_serial = entry.serial.clone();
                }
                if entry.state == "device" {
                    g.state = "device".into();
                }
            }
            None => groups.push(Group {
                primary_serial: if entry.is_wifi {
                    ro.unwrap_or_else(|| entry.serial.clone())
                } else {
                    entry.serial.clone()
                },
                model: entry.model,
                has_usb: !entry.is_wifi,
                has_wifi: entry.is_wifi,
                state: entry.state,
                all_serials: vec![entry.serial],
            }),
        }
    }

    let mut devices = Vec::new();
    for g in groups {
        let prefers_wifi = cfg
            .tablets
            .get(&g.primary_serial)
            .and_then(|p| p.preferred_transport.as_deref())
            == Some("wifi");
        let transport = match (g.has_usb, g.has_wifi) {
            (true, true) if prefers_wifi => "Wi-Fi",
            (true, _) => "USB",
            (false, true) => "Wi-Fi",
            (false, false) => "Disconnected",
        };
        let device_version = if g.state == "device" {
            installed_app_version(&g.all_serials).await
        } else {
            None
        };
        let paired = g.all_serials.iter().any(|s| cfg.tablets.contains_key(s))
            || cfg.tablets.contains_key(&g.primary_serial);
        devices.push(ConnectedDevice {
            serial: g.primary_serial,
            model: g.model,
            state: g.state,
            transport: transport.into(),
            paused: false,
            device_version,
            paired,
            install: None,
        });
    }

    for (saved_serial, profile) in &cfg.tablets {
        // Legacy profiles were keyed by "ip:port".
        if saved_serial.contains(':') {
            continue;
        }
        let label = norm_model(&profile.label);
        let present = devices.iter().any(|d| {
            d.serial == *saved_serial || (!label.is_empty() && norm_model(&d.model) == label)
        });
        if !present {
            devices.push(ConnectedDevice {
                serial: saved_serial.clone(),
                model: if profile.label.is_empty() {
                    saved_serial.clone()
                } else {
                    profile.label.clone()
                },
                state: "offline".into(),
                transport: "Disconnected".into(),
                paused: false,
                device_version: profile.app_version.clone(),
                paired: true,
                install: None,
            });
        }
    }

    devices.sort_by(|a, b| {
        let (a_on, b_on) = (a.transport != "Disconnected", b.transport != "Disconnected");
        b_on.cmp(&a_on).then_with(|| a.model.cmp(&b.model))
    });
    devices
}
