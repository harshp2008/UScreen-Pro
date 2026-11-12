//! Mapping the virtual input devices onto the right screen.
//!
//! A touchscreen or pen is only useful when the compositor maps it onto the output it belongs to;
//! otherwise a tap on the tablet lands on the primary monitor. Each compositor (KWin, Hyprland,
//! GNOME, X11) has its own way to express this, so each gets its own function here.

use super::*;

/// D-Bus interface name of a KWin input device.
pub(super) const KWIN_INPUT_IFACE: &str = "org.kde.KWin.InputDevice";

// PLATFORM(kde): finds the laptop panel with kscreen-doctor
/// The screen the user is actually looking at: the first enabled output that is
/// not one of ours. In pen-only mode the tablet drives this one, so the pen has
/// to be mapped onto it rather than onto the virtual display.
pub(super) async fn primary_non_evdi_output() -> Option<String> {
    let evdi: Vec<String> = crate::display::vdisplay::evdi_connectors()
        .into_iter()
        .map(|c| c.name)
        .collect();
    let out = tokio::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .await
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let outputs = v.get("outputs")?.as_array()?;
    let mut fallback = None;
    // Plasma 6 dropped the boolean `primary` for a `priority` where 1 is the
    // primary screen; looking only for `primary` found none and took the first
    // enabled output in the list, which on a multi-monitor desktop is not
    // necessarily the one in front of the user.
    let mut best: Option<(u64, String)> = None;
    for o in outputs {
        let name = o.get("name").and_then(|v| v.as_str())?.to_string();
        if evdi.contains(&name) || !o.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if o.get("primary").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Some(name);
        }
        if let Some(p) = o.get("priority").and_then(|v| v.as_u64()) {
            if best.as_ref().is_none_or(|(bp, _)| p < *bp) {
                best = Some((p, name.clone()));
            }
        }
        fallback.get_or_insert(name);
    }
    best.map(|(_, n)| n).or(fallback)
}

// PLATFORM(kde): reads a property of a KWin input device over D-Bus
/// Read one property of a KWin input device by its sysname.
pub(super) async fn kwin_device_property(sysname: &str, property: &str) -> Option<String> {
    crate::desktop::kde::display::get_property(
        &format!("/org/kde/KWin/InputDevice/{}", sysname),
        KWIN_INPUT_IFACE,
        property,
    )
    .await
}

// PLATFORM(linux): if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case)
// PLATFORM(linux): if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case)
// PLATFORM(linux): if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case)
// PLATFORM(linux): if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case)
/// Pin the virtual input devices to the virtual display.
///
/// An absolute-positioning device is meaningless without knowing which screen
/// it addresses. Left unmapped, libinput spreads it across the whole desktop:
/// touching the middle of the tablet lands the cursor somewhere on the laptop
/// panel, and drawing with the pen goes to the wrong monitor entirely.
///
/// This is done over KWin's D-Bus interface rather than by writing kcminputrc.
/// Writing the config file looks like the obvious route and does produce the
/// documented `[Libinput][vendor][product][name] OutputName=` entry, but KWin
/// does not apply it to these devices — verified by reading the property back
/// and finding it empty, both when written before and after device creation.
/// Setting the property directly takes effect immediately, and KWin persists it
/// itself.
pub(super) async fn map_devices_to_output(
    pen_only: bool,
    ident: &DeviceIdentity,
    card: Option<u32>,
) -> Option<String> {
    if crate::desktop::hyprland::display::active() {
        return map_devices_on_hyprland(pen_only, ident, card).await;
    }
    if crate::desktop::gnome::display::active() {
        return map_devices_on_gnome(pen_only, ident, card).await;
    }
    if crate::desktop::x11::display::active() {
        return map_devices_on_x11(pen_only, ident, card).await;
    }
    let Some(output) = target_output(pen_only, card, std::time::Duration::from_secs(10)).await
    else {
        if pen_only {
            warn!("No physical output found — pen will address the whole desktop");
        } else {
            warn!("No EVDI output found — touch and pen will address the whole desktop");
        }
        return None;
    };

    // KWin registers a device slightly after uinput creates it, so retry
    // rather than racing it. The same loop also covers a mapping that KWin
    // accepted but did not keep, which is what the read-back below catches.
    for attempt in 0..20 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        let Some(devices) = crate::desktop::kde::display::list_strings(
            "/org/kde/KWin/InputDevice",
            "org.kde.KWin.InputDeviceManager",
            "devicesSysNames",
        )
        .await
        else {
            warn!("KWin did not answer — input devices stay unmapped");
            return None;
        };

        let mut mapped = 0;
        for sysname in devices {
            let Some(name) = kwin_device_property(&sysname, "name").await else {
                continue;
            };
            if !ident.owns(&name) {
                continue;
            }
            let ok = crate::desktop::kde::display::set_property(
                &format!("/org/kde/KWin/InputDevice/{}", sysname),
                KWIN_INPUT_IFACE,
                "outputName",
                "s",
                &output,
            )
            .await;
            match ok {
                true => {
                    // A successful Set is not proof: KWin answers ok and then
                    // keeps the old value when the output is not usable yet.
                    // Only what reads back counts, so a lost mapping is
                    // retried on the next pass instead of logged as done.
                    match kwin_device_property(&sysname, "outputName").await {
                        Some(now) if now == output => {
                            info!("Mapped '{}' ({}) to output {}", name, sysname, output);
                            mapped += 1;
                        }
                        now => warn!(
                            "Mapping '{}' to {} did not take (KWin reports {:?}) — retrying",
                            name,
                            output,
                            now.unwrap_or_default()
                        ),
                    }
                }
                false => warn!(
                    "Could not map '{}': KWin refused the outputName property",
                    name
                ),
            }
        }

        if mapped >= ident.device_count {
            return Some(output);
        }
    }

    warn!("Input devices did not appear in KWin within 5s — mapping skipped");
    Some(output)
}

/// The Hyprland counterpart of the KWin mapping above. Only touch and pen are
/// pinned: the pointer device is handled by `pointer_area` either way.
pub(super) async fn map_devices_on_hyprland(
    pen_only: bool,
    ident: &DeviceIdentity,
    card: Option<u32>,
) -> Option<String> {
    let connectors = crate::display::vdisplay::evdi_connectors();
    let output = if pen_only {
        let all: Vec<String> = connectors.into_iter().map(|c| c.name).collect();
        crate::desktop::hyprland::display::primary_output(&all).await
    } else {
        let mine: Vec<String> = connectors
            .into_iter()
            .filter(|c| card.is_none_or(|want| c.card == want))
            .map(|c| c.name)
            .collect();
        crate::desktop::hyprland::display::wait_on(&mine, std::time::Duration::from_secs(10)).await
    };
    let Some(output) = output else {
        warn!("Hyprland has no output to map onto — touch and pen will address the whole desktop");
        return None;
    };
    let names = [ident.touch.as_str(), ident.pen.as_str()];
    for attempt in 0..20 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        if crate::desktop::hyprland::display::map_devices(&names, &output).await >= names.len() {
            return Some(output);
        }
    }
    warn!("Hyprland did not list both input devices within 5s — mapping incomplete");
    Some(output)
}

/// GNOME on Wayland: per-device settings keyed on the EDID triple; see
/// `desktop/gnome/display.rs`. The plain pointer has no such setting, so it keeps whatever
/// GNOME does with an absolute pointer.
pub(super) async fn map_devices_on_gnome(
    pen_only: bool,
    ident: &DeviceIdentity,
    card: Option<u32>,
) -> Option<String> {
    let mine: Vec<String> = crate::display::vdisplay::evdi_connectors()
        .into_iter()
        .filter(|c| card.is_none_or(|want| c.card == want))
        .map(|c| c.name)
        .collect();
    crate::desktop::gnome::display::map_devices(
        UINPUT_VENDOR,
        ident.product_touch,
        ident.product_pen,
        &mine,
        pen_only,
    )
    .await
}

/// Real X11 sessions other than Plasma: `xinput map-to-output`; see `desktop/x11/display.rs`.
pub(super) async fn map_devices_on_x11(
    pen_only: bool,
    ident: &DeviceIdentity,
    card: Option<u32>,
) -> Option<String> {
    let connectors = crate::display::vdisplay::evdi_connectors();
    let output = if pen_only {
        let all: Vec<String> = connectors.into_iter().map(|c| c.name).collect();
        crate::desktop::x11::display::primary_output(&all).await
    } else {
        let mine: Vec<String> = connectors
            .into_iter()
            .filter(|c| card.is_none_or(|want| c.card == want))
            .map(|c| c.name)
            .collect();
        crate::desktop::x11::display::wait_active(&mine, std::time::Duration::from_secs(10)).await
    };
    let Some(output) = output else {
        warn!("X11 has no output to map onto — touch and pen will address the whole desktop");
        return None;
    };
    // Muffin can overwrite a direct XInput transform with its own guess, so on Cinnamon the
    // explicit per-device EDID mapping is written as well (cleared again in pen-only mode).
    if std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase()
        .contains("CINNAMON")
        && !crate::desktop::gnome::display::map_cinnamon_devices(
            UINPUT_VENDOR,
            ident.product_touch,
            ident.product_pen,
            (!pen_only).then_some(output.as_str()),
        )
        .await
    {
        warn!("Cinnamon did not accept its native input mapping; using XInput mapping");
    }
    let mut names = vec![ident.touch.as_str(), ident.pen.as_str()];
    if ident.device_count > 2 {
        names.push(ident.pointer.as_str());
    }
    for attempt in 0..20 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        if crate::desktop::x11::display::map_devices(&names, &output).await >= names.len() {
            return Some(output);
        }
    }
    warn!("xinput did not list and accept every input device within 5s — mapping incomplete");
    Some(output)
}

// PLATFORM(linux): area lookup differs per desktop: Hyprland, GNOME, X11 (none) and kscreen-doctor for the rest
/// Map, then tell the pointer device where that output sits on the desktop.
pub(super) async fn map_and_note_area(
    instance: u32,
    pen_only: bool,
    ident: &DeviceIdentity,
    card: Option<u32>,
    devices: &Arc<std::sync::Mutex<InjectDevices>>,
) {
    // The interface scale comes first: it changes the size of the display on the desktop, which the pointer area below reads back.
    if !pen_only && crate::desktop::scale::supported() {
        let percent = crate::config::options::current_options(instance).interface_scale;
        crate::desktop::scale::apply(&crate::desktop::scale::Target { instance, card }, percent)
            .await;
    }
    let output = map_devices_to_output(pen_only, ident, card).await;
    let area = match output.as_deref() {
        Some(o) if crate::desktop::hyprland::display::active() => {
            crate::desktop::hyprland::display::output_area(o).await
        }
        Some(o) if crate::desktop::gnome::display::active() => {
            crate::desktop::gnome::display::output_area(o).await
        }
        Some(_) if crate::desktop::x11::display::active() => None,
        Some(o) => output_area_on_desktop(o).await,
        None => None,
    };
    let snap_grid = if crate::desktop::gnome::display::active() {
        crate::desktop::gnome::display::desktop_pixels().await
    } else {
        None
    };
    if let Ok(mut guard) = devices.lock() {
        guard.pointer_area = area;
        guard.snap_grid = snap_grid;
    }
    if let Some((w, h)) = snap_grid {
        info!("Pointer snapped to whole pixels of a {}x{} desktop", w, h);
    }
    if let Some((x, y, w, h)) = area {
        info!(
            "Pointer area on the desktop: {:.0}%,{:.0}% + {:.0}%x{:.0}%",
            x * 100.0,
            y * 100.0,
            w * 100.0,
            h * 100.0
        );
    }
}

// PLATFORM(kde): output rectangle from kscreen-doctor JSON
/// Where `output` sits on the desktop, as fractions of the whole desktop:
/// (x, y, width, height). `None` when the layout cannot be read.
///
/// Used only for the plain pointer device — see `InjectDevices::pointer_area`.
pub(super) async fn output_area_on_desktop(output: &str) -> Option<(f64, f64, f64, f64)> {
    let outputs = kscreen_outputs().await?;
    let logical = |o: &serde_json::Value| -> Option<(f64, f64, f64, f64)> {
        let scale = o
            .get("scale")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .max(0.01);
        let id = o.get("currentModeId")?.as_str()?;
        let mode = o
            .get("modes")?
            .as_array()?
            .iter()
            .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(id))?;
        let w = mode.pointer("/size/width")?.as_f64()? / scale;
        let h = mode.pointer("/size/height")?.as_f64()? / scale;
        let x = o.pointer("/pos/x")?.as_f64()?;
        let y = o.pointer("/pos/y")?.as_f64()?;
        Some((x, y, w, h))
    };
    let mut mine = None;
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for o in &outputs {
        if !o.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some((x, y, w, h)) = logical(o) else {
            continue;
        };
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
        if o.get("name").and_then(|v| v.as_str()) == Some(output) {
            mine = Some((x, y, w, h));
        }
    }
    let (x, y, w, h) = mine?;
    (max_x > 0.0 && max_y > 0.0).then(|| (x / max_x, y / max_y, w / max_x, h / max_y))
}

// PLATFORM(kde): runs kscreen-doctor -j and parses its JSON
/// The outputs KWin currently knows, as reported by `kscreen-doctor -j`.
/// `None` when the tool is not there at all (not a KDE session).
pub(super) async fn kscreen_outputs() -> Option<Vec<serde_json::Value>> {
    let out = tokio::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .await
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(v.get("outputs")?.as_array()?.clone())
}

/// The output the devices should address in this mode, returned only once
/// KWin lists it as enabled.
///
/// Mapping is not something that can be done ahead of time: set `outputName`
/// while the virtual display is still being brought back and KWin keeps the
/// previous mapping, so after leaving graphics-tablet mode the pen and touch
/// would go on driving the laptop screen (issue #6). Leaving display mode has
/// the opposite problem — the physical screen is always there, but the EVDI
/// output is going away at that moment. So this polls the output list up to
/// `timeout` and only then falls back to the best name it knows, so a mapping
/// is at least attempted on a desktop where kscreen-doctor cannot answer.
pub(super) async fn target_output(
    pen_only: bool,
    card: Option<u32>,
    timeout: std::time::Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if pen_only {
            if let Some(name) = primary_non_evdi_output().await {
                return Some(name);
            }
            // No kscreen-doctor means no KDE session: nothing to wait for.
            kscreen_outputs().await?;
        } else {
            let connectors = crate::display::vdisplay::evdi_connectors();
            // This tablet's own card first; "any connected EVDI output" only
            // as a fallback while the card is not known yet.
            let fallback = connectors
                .iter()
                .find(|c| card.is_some_and(|want| c.card == want))
                .or_else(|| connectors.iter().find(|c| c.connected))
                .or_else(|| connectors.first())
                .map(|c| c.name.clone());
            let Some(outputs) = kscreen_outputs().await else {
                return fallback;
            };
            let mine: Vec<&str> = connectors
                .iter()
                .filter(|c| card.is_none_or(|want| c.card == want))
                .map(|c| c.name.as_str())
                .collect();
            let enabled = outputs.iter().find(|o| {
                let name = o.get("name").and_then(|v| v.as_str()).unwrap_or("");
                mine.contains(&name) && o.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false)
            });
            if let Some(o) = enabled {
                return o.get("name").and_then(|v| v.as_str()).map(str::to_string);
            }
            if tokio::time::Instant::now() >= deadline {
                if fallback.is_some() {
                    warn!(
                        "EVDI output not enabled within {:?} — mapping onto {:?} anyway",
                        timeout, fallback
                    );
                }
                return fallback;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
