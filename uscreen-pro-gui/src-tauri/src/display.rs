//! What the desktop says about the virtual displays, for the Interface size setting.

/// The scale GNOME would pick on its own for a UScreen virtual display, in percent, so "Custom" can start where "Automatic" already is
/// and picking it changes nothing on screen. [width] and [height] (a device's panel size, if known) pick the display with that mode;
/// without them the first UScreen display is used. `None` when the layout cannot be read or no such display is up.
fn auto_percent_from(json: &str, width: Option<u32>, height: Option<u32>) -> Option<u32> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    for m in v.get("data")?.get(1)?.as_array()? {
        if m.get(0)?.get(1)?.as_str()? != "USC" {
            continue;
        }
        for mode in m.get(1)?.as_array()? {
            let current = mode
                .get(6)
                .and_then(|p| p.get("is-current"))
                .and_then(|c| c.get("data").or(Some(c)))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            if !current {
                continue;
            }
            let (w, h) = (mode.get(1)?.as_u64()? as u32, mode.get(2)?.as_u64()? as u32);
            if width.is_some_and(|x| x != w) || height.is_some_and(|y| y != h) {
                continue;
            }
            return Some((mode.get(4)?.as_f64()? * 100.0).round() as u32);
        }
    }
    None
}

/// Tauri command: the automatic interface scale in percent of the virtual display of a device with this panel size.
#[tauri::command]
pub async fn display_auto_percent(width: Option<u32>, height: Option<u32>) -> Option<u32> {
    let out = tokio::process::Command::new("busctl")
        .args([
            "--user",
            "--json=short",
            "call",
            "org.gnome.Mutter.DisplayConfig",
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
        ])
        .output()
        .await
        .ok()?;
    auto_percent_from(&String::from_utf8_lossy(&out.stdout), width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = r#"{"data":[3,[
      [["eDP-1","AUO","0x6db2","0x0"],[["1920x1080@60.000",1920,1080,60.0,1.0,[1.0],{"is-current":{"type":"b","data":true}}]],{}],
      [["DVI-I-1","USC","UScreen","0x00000001"],[["2340x1080@90.000",2340,1080,90.0,2.5,[1.0,2.5],{"is-current":{"type":"b","data":true}}]],{}],
      [["DVI-I-2","USC","UScreen","0x00000002"],[["2560x1600@90.000",2560,1600,90.0,1.748633861541748,[1.0,1.7486],{"is-current":{"type":"b","data":true}}]],{}]],[],{}]}"#;

    /// The panel size picks the display; without it the first UScreen display answers; the laptop is never one.
    #[test]
    fn the_matching_display_gives_its_preferred_percent() {
        assert_eq!(auto_percent_from(STATE, Some(2560), Some(1600)), Some(175));
        assert_eq!(auto_percent_from(STATE, Some(2340), None), Some(250));
        assert_eq!(auto_percent_from(STATE, None, None), Some(250));
        assert_eq!(auto_percent_from(STATE, Some(800), Some(600)), None);
        assert_eq!(auto_percent_from("not json", None, None), None);
    }
}
