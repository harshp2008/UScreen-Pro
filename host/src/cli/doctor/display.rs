//! Checks of the virtual display and the desktop around it: compositor, on-screen keyboard, autostart, colour

use super::*;

// PLATFORM(linux): if-ladder over desktop backends for the "is the virtual display on" report
/// Doctor check of the EVDI virtual display: its connector, mode and placement.
pub(super) async fn check_virtual_display(r: &mut Report, cfg: &FileConfig) {
    let connectors = vdisplay::evdi_connectors();
    if connectors.is_empty() {
        r.line(Level::Fail, "EVDI connector", "none found in sysfs");
        r.hint("the evdi module is loaded but exposes no DRM connector — reboot or re-add");
        return;
    }
    for c in &connectors {
        r.line(
            if c.connected { Level::Ok } else { Level::Warn },
            "EVDI connector",
            &format!(
                "{} ({})",
                c.name,
                if c.connected {
                    "connected"
                } else {
                    "disconnected — helper not running"
                }
            ),
        );
    }

    let names: Vec<&str> = connectors.iter().map(|c| c.name.as_str()).collect();
    if crate::desktop::hyprland::display::active() {
        let Some(monitors) = crate::desktop::hyprland::display::monitors().await else {
            r.line(Level::Warn, "Hyprland", "hyprctl did not answer");
            return;
        };
        for m in monitors.iter().filter(|m| names.contains(&m.name.as_str())) {
            if m.is_on() {
                r.line(
                    Level::Ok,
                    "Hyprland output",
                    &format!("{} at {}x{}, scale {}", m.name, m.width, m.height, m.scale),
                );
            } else {
                r.line(
                    Level::Warn,
                    "Hyprland output",
                    &format!("{} is off (0x0)", m.name),
                );
                r.hint("the daemon switches it on while a tablet is a screen; nothing is rendered while it is off");
            }
        }
        return;
    }
    let Some(json) = output_of("kscreen-doctor", &["-j"]).await else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
        r.line(Level::Warn, "kscreen-doctor", "unparseable JSON output");
        return;
    };
    let Some(outputs) = value.get("outputs").and_then(|o| o.as_array()) else {
        return;
    };

    for out in outputs {
        let name = out.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if !names.contains(&name) {
            continue;
        }
        let enabled = out
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !enabled {
            r.line(Level::Warn, "KDE output", &format!("{} is disabled", name));
            r.hint("the daemon enables it on start; nothing is rendered while it is off");
            continue;
        }
        let w = out
            .pointer("/size/width")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let h = out
            .pointer("/size/height")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        // A mismatch here is the classic "skewed / torn picture": ffmpeg is
        // told one frame size while the helper produces another.
        if w as u32 == cfg.width && h as u32 == cfg.height {
            r.line(
                Level::Ok,
                "KDE output mode",
                &format!("{} at {}x{}", name, w, h),
            );
        } else {
            r.line(
                Level::Fail,
                "KDE output mode",
                &format!(
                    "{} is {}x{}, config says {}x{}",
                    name, w, h, cfg.width, cfg.height
                ),
            );
            r.hint("the encoder frame size will not match the capture — picture will be skewed");
        }
    }
}

// PLATFORM(kde): parses `kwin_wayland --version` style banner text
/// `kwin 6.8.2` (the output of `kwin_wayland --version`) as (6, 8, 2).
pub(super) fn kwin_version(text: &str) -> Option<(u32, u32, u32)> {
    let v = text.split_whitespace().last()?;
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

// PLATFORM(kde): on-screen keyboard check; says nothing useful off KDE
/// The on-screen keyboard pops up whenever a touchscreen is used, and the
/// tablet's touch device is exactly that as far as the desktop is concerned.
/// Reported because it is a global desktop setting, not something the daemon
/// should quietly decide on the user's behalf.
pub(super) async fn check_osk(r: &mut Report) {
    // Hyprland has no KWin to ask; hyprctl does the mapping there.
    if crate::desktop::hyprland::display::active() {
        r.line(
            Level::Ok,
            "Hyprland",
            "hyprctl maps touch and pen onto the tablet's output",
        );
        return;
    }
    if crate::desktop::gnome::display::active() {
        r.line(
            Level::Ok,
            "GNOME",
            "pen and touch are mapped through GNOME's per-device settings",
        );
        return;
    }
    if crate::desktop::x11::display::active() {
        r.line(
            Level::Ok,
            "X11",
            "pen and touch are mapped with xinput map-to-output",
        );
        return;
    }
    // KWin is only there on Plasma. On COSMIC or GNOME its absence is not a
    // fault, and a FAIL sent people hunting for a qdbus package (#26).
    let session = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !session.is_empty() && !session.to_uppercase().contains("KDE") {
        r.line(
            Level::Warn,
            "desktop",
            &format!("{} — pen and touch mapping is manual here", session),
        );
        if std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "x11") && !command_exists("xinput")
        {
            r.hint("install the `xinput` package and UScreen maps the pen and touch itself on X11");
        } else {
            r.hint(
                "UScreen maps the tablet's input onto its screen automatically on KDE Plasma, \
                 Hyprland, GNOME (Wayland) and X11 sessions. Elsewhere, assign the \"UScreen \
                 Pen\" and \"UScreen Touch\" devices to the UScreen output in your desktop's \
                 settings.",
            );
        }
        return;
    }
    // Whether we can reach KWin at all decides whether touch and pen land on
    // the tablet's screen, so it is reported first and in its own right.
    match crate::desktop::kde::display::backend().await {
        Some(b) => r.line(
            Level::Ok,
            "KWin D-Bus",
            &format!("reachable via {}", b.name()),
        ),
        None => {
            r.line(
                Level::Fail,
                "KWin D-Bus",
                "unreachable — touch and pen will drive the wrong screen",
            );
            r.hint(
                "the daemon maps the tablet's input onto the virtual display over KWin's \
                 D-Bus interface, using busctl (systemd) or qdbus. Install systemd's busctl, \
                 or a qdbus package (qdbus-qt6 on Debian/Ubuntu, qt6-tools elsewhere).",
            );
            return;
        }
    }

    // KWin 6.8.2 crashes the whole desktop when a virtual (EVDI) screen is
    // shown, which is what UScreen's screen is to it (KDE bug 527984).
    if let Some(text) = output_of("kwin_wayland", &["--version"]).await {
        match kwin_version(&text) {
            Some((6, 8, 2)) => {
                r.line(
                    Level::Warn,
                    "KWin version",
                    "6.8.2 — unpatched builds crash with virtual displays (KDE bug 527984)",
                );
                r.hint(
                    "if the desktop crashes when the tablet connects: update KWin. Patched 6.8.2 \
                     packages exist (Arch's 6.8.2-2 has the fix); if yours is one, ignore this. \
                     Otherwise go back to 6.8.1",
                );
            }
            Some((a, b, c)) => r.line(Level::Ok, "KWin version", &format!("{}.{}.{}", a, b, c)),
            None => {}
        }
    }

    // The live value, not the one in kwinrc: KWin does not re-read that file,
    // so the two disagree routinely and only this one reflects what happens.
    let Some(raw) = crate::desktop::kde::display::get_property(
        "/VirtualKeyboard",
        "org.kde.kwin.VirtualKeyboard",
        "mode",
    )
    .await
    else {
        return;
    };
    let mode: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    match mode.trim() {
        "1" | "2" => {
            r.line(Level::Warn, "on-screen keyboard", "pops up on touch input");
            r.hint("the daemon turns this off while it runs and puts it back on exit");
        }
        "0" => r.line(Level::Ok, "on-screen keyboard", "only when asked for"),
        _ => {}
    }
}

// PLATFORM(systemd): asks systemctl --user whether uscreen.service is enabled
/// Whether plugging the cable in is actually enough on its own.
pub(super) async fn check_autostart(r: &mut Report) {
    match output_of("systemctl", &["--user", "is-enabled", "uscreen.service"]).await {
        Some(v) if v.trim() == "enabled" => {
            r.line(Level::Ok, "start with the desktop", "enabled");
        }
        Some(_) => {
            r.line(
                Level::Warn,
                "start with the desktop",
                "disabled — the daemon must be started by hand",
            );
            r.hint("systemctl --user enable --now uscreen.service");
        }
        None => {}
    }
}

/// Colour accuracy, for using the tablet to judge images rather than just to
/// hold windows. Everything here is a setting rather than a bug, but each one
/// silently ruins colour and none of them is visible from the host side.
pub(super) async fn check_colour(r: &mut Report) {
    // Eye comfort / blue light filter warms the whole panel. Nothing on the
    // host can compensate, and it is easy to leave on by accident.
    match output_of(
        "adb",
        &["shell", "settings", "get", "system", "blue_light_filter"],
    )
    .await
    {
        Some(v) if v.trim() == "1" => {
            r.line(Level::Fail, "blue light filter", "ON — colours are warmed");
            r.hint("tablet: Settings → Display → Eye comfort shield → off");
        }
        Some(v) if v.trim() == "0" => r.line(Level::Ok, "blue light filter", "off"),
        _ => {}
    }

    // The same warming from other makers. Stock Android's Night Light is
    // often on a schedule, which is how a picture can look right in the
    // afternoon and orange after a reboot in the evening (#29); Xiaomi's
    // Reading mode does the same.
    for (namespace, key, name, hint) in [
        (
            "secure",
            "night_display_activated",
            "Night Light",
            "tablet: Settings → Display → Night Light → off (or its schedule)",
        ),
        (
            "system",
            "screen_paper_mode_enabled",
            "reading mode",
            "tablet: Settings → Display → Reading mode → off",
        ),
    ] {
        if let Some(v) = output_of("adb", &["shell", "settings", "get", namespace, key]).await {
            if v.trim() == "1" {
                r.line(Level::Fail, name, "ON — colours are warmed");
                r.hint(hint);
            }
        }
    }

    // Samsung's "Vivid" screen mode stretches saturation past sRGB. "Natural"
    // is the colour-accurate one.
    if let Some(v) = output_of(
        "adb",
        &["shell", "settings", "get", "system", "screen_mode_setting"],
    )
    .await
    {
        let v = v.trim().to_string();
        if v == "2" {
            r.line(Level::Ok, "tablet screen mode", "Natural (sRGB)");
        } else if !v.is_empty() && v != "null" {
            r.line(
                Level::Warn,
                "tablet screen mode",
                &format!("{} — not the sRGB-accurate mode", v),
            );
            r.hint("tablet: Settings → Display → Screen mode → Natural");
        }
    }

    // Samsung's "Motion smoothness" setting. On "Standard" the panel only
    // offers apps its 60 Hz modes, so the app's request for the fastest one
    // gets 60 and every frame waits an average of 8 ms for vsync instead of
    // 4. Seen on a Tab S9 Ultra: the app asked for the best mode and was
    // handed 60 Hz until this was switched to Adaptive.
    if let Some(v) = output_of(
        "adb",
        &["shell", "settings", "get", "secure", "refresh_rate_mode"],
    )
    .await
    {
        let v = v.trim().to_string();
        if v == "0" {
            r.line(
                Level::Warn,
                "tablet refresh rate",
                "Motion smoothness is Standard — the panel is held at 60 Hz",
            );
            r.hint("tablet: Settings → Display → Motion smoothness → Adaptive (120 Hz)");
        } else if !v.is_empty() && v != "null" {
            r.line(
                Level::Ok,
                "tablet refresh rate",
                "Motion smoothness: adaptive",
            );
        }
    }

    // KWin can colour-manage the virtual output, but only once a profile is
    // attached to it.
    let names: Vec<String> = vdisplay::evdi_connectors()
        .into_iter()
        .map(|c| c.name)
        .collect();
    if let Some(json) = output_of("kscreen-doctor", &["-j"]).await {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
            for out in v
                .get("outputs")
                .and_then(|o| o.as_array())
                .unwrap_or(&vec![])
            {
                let name = out.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if !names.iter().any(|n| n == name) {
                    continue;
                }
                let icc = out
                    .get("iccProfilePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if icc.is_empty() {
                    r.line(
                        Level::Warn,
                        "colour profile",
                        "none assigned to the virtual display",
                    );
                    r.hint(
                        "System Settings → Display → pick the UScreen display → Color Profile. \
                         A generic sRGB profile is the right baseline; a measured one needs a \
                         colorimeter pointed at the tablet.",
                    );
                } else {
                    r.line(Level::Ok, "colour profile", icc);
                }
            }
        }
    }
}
