//! GNOME on Wayland: pin the tablet's pen and touch to the virtual output.
//!
//! GNOME has nothing like KWin's D-Bus property. What it has is per-device
//! settings — `org.gnome.desktop.peripherals.tablet` and `.touchscreen`,
//! relocatable schemas whose `output` key holds the monitor's EDID triple
//! (vendor, product, serial). That is what Settings → Wacom Tablet → Map to
//! Monitor writes, and mutter re-maps the device as soon as it changes.
//!
//! The triple is not worked out here: mutter is asked for the one it matches
//! on (`GetCurrentState`), so whatever it calls our monitor is what gets
//! written. GNOME already switches a new monitor on and places it itself, so
//! there is no enable step.
//!
//! Written from the schemas and mutter's documented D-Bus interface without a
//! GNOME session to run it in; every step is logged, and the setting is read
//! back like the KWin mapping is.

use tracing::{info, warn};

fn command_exists(name: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| std::path::Path::new(d).join(name).exists())
}

/// A GNOME Wayland session with the two tools this needs. On GNOME's X11
/// session `display/x11.rs` does the mapping instead.
pub fn active() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase();
    let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
    desktop.contains("GNOME") && wayland && command_exists("gsettings") && command_exists("busctl")
}

async fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `(connector, vendor, product, serial)` of every monitor, from the JSON form
/// of `GetCurrentState`: `[serial, [[[connector, vendor, product, serial],
/// modes, props], ...], logical monitors, props]`.
fn parse_monitors(json: &str) -> Option<Vec<[String; 4]>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut out = Vec::new();
    for m in v.get("data")?.get(1)?.as_array()? {
        let ids = m.get(0)?.as_array()?;
        let s = |i: usize| ids.get(i)?.as_str().map(str::to_string);
        out.push([s(0)?, s(1)?, s(2)?, s(3)?]);
    }
    Some(out)
}

async fn monitors() -> Option<Vec<[String; 4]>> {
    let json = output_of(
        "busctl",
        &[
            "--user",
            "--json=short",
            "call",
            "org.gnome.Mutter.DisplayConfig",
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
        ],
    )
    .await?;
    parse_monitors(&json)
}

/// Where `connector` sits on the desktop, as fractions of the whole desktop:
/// `(x, y, width, height)`. Used for the plain pointer device on GNOME Wayland.
pub async fn output_area(connector: &str) -> Option<(f64, f64, f64, f64)> {
    let json = output_of(
        "busctl",
        &[
            "--user",
            "--json=short",
            "call",
            "org.gnome.Mutter.DisplayConfig",
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
        ],
    )
    .await?;
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    let data = v.get("data")?.as_array()?;
    let monitors_arr = data.get(1)?.as_array()?;
    let logical_arr = data.get(2)?.as_array()?;

    // Map connector -> (physical_w, physical_h)
    let mut phys_sizes = std::collections::HashMap::new();
    for m in monitors_arr {
        let conn = m.get(0)?.get(0)?.as_str()?;
        let modes = m.get(1)?.as_array()?;
        for mode in modes {
            let mode_arr = mode.as_array()?;
            let w = mode_arr.get(1)?.as_f64()?;
            let h = mode_arr.get(2)?.as_f64()?;
            let is_cur = mode_arr
                .get(6)
                .and_then(|p| p.get("is-current"))
                .and_then(|c| c.get("data").or(Some(c)))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            if is_cur || modes.len() == 1 {
                phys_sizes.insert(conn, (w, h));
                break;
            }
        }
    }

    let mut mine = None;
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for lm in logical_arr {
        let lm_arr = lm.as_array()?;
        let x = lm_arr.first()?.as_f64()?;
        let y = lm_arr.get(1)?.as_f64()?;
        let scale = lm_arr.get(2)?.as_f64().unwrap_or(1.0).max(0.01);
        let conn = lm_arr.get(5)?.get(0)?.get(0)?.as_str()?;
        let (pw, ph) = phys_sizes.get(conn).copied().unwrap_or((1920.0, 1080.0));
        let w = pw / scale;
        let h = ph / scale;
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
        if conn == connector {
            mine = Some((x, y, w, h));
        }
    }

    let (x, y, w, h) = mine?;
    (max_x > 0.0 && max_y > 0.0).then(|| (x / max_x, y / max_y, w / max_x, h / max_y))
}

#[derive(Clone, Copy)]
enum Kind {
    Tablet,
    Touchscreen,
}

/// The relocatable schema with its per-device path, which GNOME builds from
/// the device's USB ids as four lowercase hex digits each.
fn schema_with_path(kind: Kind, vendor: u16, product: u16) -> String {
    match kind {
        Kind::Tablet => format!(
            "org.gnome.desktop.peripherals.tablet:/org/gnome/desktop/peripherals/tablets/{:04x}:{:04x}/",
            vendor, product
        ),
        Kind::Touchscreen => format!(
            "org.gnome.desktop.peripherals.touchscreen:/org/gnome/desktop/peripherals/touchscreens/{:04x}:{:04x}/",
            vendor, product
        ),
    }
}

/// The GVariant text for the `output` key; `None` is the unmapped value.
fn triple_text(ids: Option<[&str; 3]>) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"));
    match ids {
        Some([a, b, c]) => format!("[{}, {}, {}]", q(a), q(b), q(c)),
        None => "['', '', '']".to_string(),
    }
}

/// Write `output` and read it back; a write that did not stick is not done.
async fn set_output(schema_path: &str, text: &str) -> bool {
    if output_of("gsettings", &["set", schema_path, "output", text])
        .await
        .is_none()
    {
        warn!("gsettings refused {} output {}", schema_path, text);
        return false;
    }
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    match output_of("gsettings", &["get", schema_path, "output"]).await {
        Some(now) if squash(&now) == squash(text) => true,
        now => {
            warn!(
                "{} output reads back as {:?}, wanted {}",
                schema_path, now, text
            );
            false
        }
    }
}

/// Map the pen and touch devices to the first of `connectors` mutter lists,
/// or — in graphics-tablet mode, where the pen drives the existing screens —
/// clear the mapping so a stale one cannot point at an output that is gone.
/// Returns the connector it mapped to.
pub async fn map_devices(
    vendor: u16,
    touch_product: u16,
    pen_product: u16,
    connectors: &[String],
    pen_only: bool,
) -> Option<String> {
    let mut target: Option<[String; 4]> = None;
    if !pen_only {
        for attempt in 0..40 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            let Some(list) = monitors().await else {
                continue;
            };
            target = list.into_iter().find(|m| connectors.contains(&m[0]));
            if target.is_some() {
                break;
            }
        }
        if target.is_none() {
            warn!("mutter does not list the EVDI output — touch and pen stay unmapped");
            return None;
        }
    }
    let text = match &target {
        Some(m) => triple_text(Some([m[1].as_str(), m[2].as_str(), m[3].as_str()])),
        None => triple_text(None),
    };
    let tablet = set_output(&schema_with_path(Kind::Tablet, vendor, pen_product), &text).await;
    let touch = set_output(
        &schema_with_path(Kind::Touchscreen, vendor, touch_product),
        &text,
    )
    .await;
    match (&target, tablet && touch) {
        (Some(m), true) => info!("Mapped pen and touch to {} ({})", m[0], text),
        (None, true) => info!("Cleared the pen and touch mapping"),
        _ => {
            warn!("GNOME did not take the pen/touch mapping — assign it in Settings → Wacom Tablet")
        }
    }
    target.map(|m| m[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_edid_triple_from_getcurrentstate_json() {
        let json = r#"{"type":"u(a((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv})",
            "data":[7,
              [[["DVI-I-1","USC","UScreen","1"],
                [["2960x1848@90.000",2960,1848,90.0,90.0,[1.0],{"is-current":{"type":"b","data":true}}]],{}],
               [["eDP-1","BOE","0x0a1b","0x00000000"],[],{}]],
              [],{}]}"#;
        let m = parse_monitors(json).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], ["DVI-I-1", "USC", "UScreen", "1"]);
        assert_eq!(m[1][0], "eDP-1");
        assert!(parse_monitors("{}").is_none());
    }

    #[test]
    fn builds_the_per_device_schema_path_and_value() {
        assert_eq!(
            schema_with_path(Kind::Tablet, 0x4553, 0x0002),
            "org.gnome.desktop.peripherals.tablet:/org/gnome/desktop/peripherals/tablets/4553:0002/"
        );
        assert!(schema_with_path(Kind::Touchscreen, 0x4553, 0x0001)
            .ends_with("/touchscreens/4553:0001/"));
        assert_eq!(
            triple_text(Some(["USC", "UScreen", "1"])),
            "['USC', 'UScreen', '1']"
        );
        assert_eq!(triple_text(Some(["A'B", "x", "y"])), "['A\\'B', 'x', 'y']");
        assert_eq!(triple_text(None), "['', '', '']");
    }
}
