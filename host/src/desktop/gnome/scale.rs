//! The interface scale of a UScreen virtual display on GNOME Wayland, set through Mutter's `DisplayConfig` service.
//!
//! The scale GNOME gives a new monitor comes from its size and resolution, and GNOME then remembers what it chose, so nothing we put in
//! the EDID changes it reliably. Mutter's `ApplyMonitorsConfig` does: the same call the Settings app makes. The monitors to the right of
//! the one that changes are moved so the layout keeps no gap and no overlap.
//!
//! The layout is applied as a temporary configuration (method 1): Mutter asks "Keep changes?" only for a persistent one, and a scale that is
//! set again from the settings every time the display is created does not need to be remembered.
//!
//! Percent 0 means "leave it to GNOME": the monitor's preferred scale is applied, which also undoes an earlier custom value.

use std::collections::HashMap;
use tracing::{info, warn};

/// One logical monitor of the layout: where it sits, its scale, and the physical monitors (connector, mode id) it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Logical {
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: u32,
    pub primary: bool,
    pub monitors: Vec<(String, String)>,
}

/// What a physical monitor offers in its current mode.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeInfo {
    /// Width in physical pixels.
    pub width: f64,
    /// The scale GNOME would pick on its own.
    pub preferred_scale: f64,
    /// The scales this mode supports.
    pub scales: Vec<f64>,
}

/// `GetCurrentState`, reduced to what the scale change needs.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub serial: u32,
    pub logical: Vec<Logical>,
    /// connector -> (vendor, product, serial of the monitor)
    pub ids: HashMap<String, [String; 3]>,
    /// connector -> (current mode id, what it offers)
    pub modes: HashMap<String, (String, ModeInfo)>,
}

/// Read `GetCurrentState` in busctl's JSON form: `[serial, monitors, logical monitors, props]`, where a monitor is
/// `[[connector, vendor, product, serial], modes, props]`, a mode `[id, w, h, refresh, preferred scale, [scales], props]` and a logical
/// monitor `[x, y, scale, transform, primary, [[connector, ...]], props]`.
pub fn parse_state(json: &str) -> Option<State> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let data = v.get("data")?.as_array()?;
    let serial = data.first()?.as_u64()? as u32;
    let mut ids = HashMap::new();
    let mut modes = HashMap::new();
    for m in data.get(1)?.as_array()? {
        let id = m.get(0)?.as_array()?;
        let connector = id.first()?.as_str()?.to_string();
        let s = |i: usize| id.get(i)?.as_str().map(str::to_string);
        ids.insert(connector.clone(), [s(1)?, s(2)?, s(3)?]);
        let list = m.get(1)?.as_array()?;
        let current = list
            .iter()
            .find(|mode| {
                mode.get(6)
                    .and_then(|p| p.get("is-current"))
                    .and_then(|c| c.get("data").or(Some(c)))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false)
            })
            .or(if list.len() == 1 { list.first() } else { None })?;
        let mode_id = current.get(0)?.as_str()?.to_string();
        modes.insert(
            connector,
            (
                mode_id,
                ModeInfo {
                    width: current.get(1)?.as_f64()?,
                    preferred_scale: current.get(4)?.as_f64()?,
                    scales: current
                        .get(5)?
                        .as_array()?
                        .iter()
                        .filter_map(|s| s.as_f64())
                        .collect(),
                },
            ),
        );
    }
    let mut logical = Vec::new();
    for l in data.get(2)?.as_array()? {
        let mut monitors = Vec::new();
        for mon in l.get(5)?.as_array()? {
            let connector = mon.get(0)?.as_str()?.to_string();
            let mode_id = modes.get(&connector)?.0.clone();
            monitors.push((connector, mode_id));
        }
        logical.push(Logical {
            x: l.get(0)?.as_i64()? as i32,
            y: l.get(1)?.as_i64()? as i32,
            scale: l.get(2)?.as_f64()?,
            transform: l.get(3)?.as_u64()? as u32,
            primary: l.get(4)?.as_bool()?,
            monitors,
        });
    }
    Some(State {
        serial,
        logical,
        ids,
        modes,
    })
}

/// The supported scale nearest to [wanted].
pub fn nearest_scale(wanted: f64, supported: &[f64]) -> Option<f64> {
    supported
        .iter()
        .copied()
        .min_by(|a, b| (a - wanted).abs().total_cmp(&(b - wanted).abs()))
}

/// Width of a logical monitor in logical pixels.
fn logical_width(state: &State, l: &Logical, scale: f64) -> i32 {
    let w = l
        .monitors
        .first()
        .and_then(|(c, _)| state.modes.get(c))
        .map(|(_, m)| m.width)
        .unwrap_or(0.0);
    (w / scale).round() as i32
}

/// The layout with the monitor of [serial] (an EDID serial such as `0x00000001`) at [percent] (0 = its preferred scale), the monitors
/// that sat directly right of it moved along, or `None` when it is not listed or already has that scale.
pub fn plan(state: &State, serial: &str, percent: u32) -> Option<Vec<Logical>> {
    let connector = state
        .ids
        .iter()
        .find(|(_, id)| id[0] == "USC" && id[2] == serial)
        .map(|(c, _)| c.clone())?;
    let (_, mode) = state.modes.get(&connector)?;
    let wanted = if percent == 0 {
        mode.preferred_scale
    } else {
        percent as f64 / 100.0
    };
    let scale = nearest_scale(wanted, &mode.scales)?;
    let idx = state
        .logical
        .iter()
        .position(|l| l.monitors.iter().any(|(c, _)| *c == connector))?;
    if (state.logical[idx].scale - scale).abs() < 1e-6 {
        return None;
    }
    let old_w: Vec<i32> = state
        .logical
        .iter()
        .map(|l| logical_width(state, l, l.scale))
        .collect();
    let mut out = state.logical.clone();
    out[idx].scale = scale;
    let new_w: Vec<i32> = out
        .iter()
        .map(|l| logical_width(state, l, l.scale))
        .collect();
    // Walk left to right; whatever touched its left neighbour before keeps touching it.
    let mut order: Vec<usize> = (0..out.len()).collect();
    order.sort_by_key(|&i| state.logical[i].x);
    for w in order.windows(2) {
        let (prev, i) = (w[0], w[1]);
        if state.logical[i].x == state.logical[prev].x + old_w[prev] {
            out[i].x = out[prev].x + new_w[prev];
        }
    }
    Some(out)
}

/// The `busctl call` arguments of `ApplyMonitorsConfig(serial, method, logical monitors, {})`; method 1 is temporary, 2 persistent.
pub fn busctl_args(serial: u32, method: u32, layout: &[Logical]) -> Vec<String> {
    let mut a: Vec<String> = [
        "--user",
        "call",
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
        "ApplyMonitorsConfig",
        "uua(iiduba(ssa{sv}))a{sv}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(serial.to_string());
    a.push(method.to_string());
    a.push(layout.len().to_string());
    for l in layout {
        a.extend([
            l.x.to_string(),
            l.y.to_string(),
            format!("{}", l.scale),
            l.transform.to_string(),
            l.primary.to_string(),
            l.monitors.len().to_string(),
        ]);
        for (connector, mode) in &l.monitors {
            a.extend([connector.clone(), mode.clone(), "0".into()]);
        }
    }
    a.push("0".into());
    a
}

/// Set the interface scale of the virtual display of [instance] to [percent] (0 = automatic). Returns whether the layout is now as asked
/// (also true when it already was).
pub async fn apply(instance: u32, percent: u32) -> bool {
    let get = tokio::process::Command::new("busctl")
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
        .await;
    let Some(state) = get
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_state(&String::from_utf8_lossy(&o.stdout)))
    else {
        warn!("Interface scale: could not read the monitor layout from Mutter");
        return false;
    };
    let serial = format!("0x{:08x}", instance + 1);
    let Some(layout) = plan(&state, &serial, percent) else {
        return true;
    };
    let out = tokio::process::Command::new("busctl")
        .args(busctl_args(state.serial, 1, &layout))
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            info!(
                "Interface scale of display {} set to {}",
                instance + 1,
                if percent == 0 {
                    "automatic".to_string()
                } else {
                    format!("{percent}%")
                }
            );
            true
        }
        Ok(o) => {
            warn!(
                "Interface scale: Mutter refused the layout: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            false
        }
        Err(e) => {
            warn!("Interface scale: could not run busctl: {}", e);
            false
        }
    }
}

/// Tests for reading the layout and planning a scale change.
#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed `GetCurrentState` of a laptop and a phone display (the phone at scale 2.5) and a tablet display to its right.
    const STATE: &str = r#"{"type":"ua((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv}","data":[26,
      [[["eDP-1","AUO","0x6db2","0x00000000"],[["1920x1080@60.000",1920,1080,60.0,1.0,[1.0,1.25,2.0],{"is-current":{"type":"b","data":true}}]],{}],
       [["DVI-I-1","USC","UScreen","0x00000001"],[["2340x1080@90.000",2340,1080,90.0,2.5,[1.0,1.25,1.5,2.0,2.5],{"is-current":{"type":"b","data":true}}]],{}],
       [["DVI-I-2","USC","UScreen","0x00000002"],[["2560x1600@90.000",2560,1600,90.0,1.75,[1.0,1.25,1.75,2.0],{"is-current":{"type":"b","data":true}}]],{}]],
      [[0,0,1.0,0,true,[["eDP-1","AUO","0x6db2","0x00000000"]],{}],
       [1920,0,2.5,0,false,[["DVI-I-1","USC","UScreen","0x00000001"]],{}],
       [2856,0,1.75,0,false,[["DVI-I-2","USC","UScreen","0x00000002"]],{}]],{}]}"#;

    /// The layout is read: serial, three logical monitors, the phone's mode and its scales.
    #[test]
    fn the_state_is_parsed() {
        let s = parse_state(STATE).unwrap();
        assert_eq!(s.serial, 26);
        assert_eq!(s.logical.len(), 3);
        assert_eq!(s.logical[1].scale, 2.5);
        assert_eq!(s.modes["DVI-I-1"].0, "2340x1080@90.000");
        assert_eq!(s.modes["DVI-I-1"].1.scales, vec![1.0, 1.25, 1.5, 2.0, 2.5]);
    }

    /// A percentage snaps to the nearest supported scale.
    #[test]
    fn percent_snaps_to_a_supported_scale() {
        assert_eq!(nearest_scale(1.4, &[1.0, 1.25, 1.5, 2.0]), Some(1.5));
        assert_eq!(nearest_scale(3.0, &[1.0, 2.0]), Some(2.0));
        assert_eq!(nearest_scale(1.0, &[]), None);
    }

    /// Lowering the phone to 100 % shrinks nothing on the laptop and moves the tablet next to it, with no gap.
    #[test]
    fn the_monitors_to_its_right_follow_the_changed_width() {
        let s = parse_state(STATE).unwrap();
        let p = plan(&s, "0x00000001", 100).unwrap();
        assert_eq!(p[1].scale, 1.0);
        assert_eq!(p[1].x, 1920);
        // The phone is now 2340 wide in logical pixels, so the tablet starts at 1920 + 2340.
        assert_eq!(p[2].x, 4260);
        assert_eq!(p[0].x, 0);
    }

    /// 0 means the preferred scale: it undoes a custom value and does nothing when the scale already is the preferred one.
    #[test]
    fn automatic_uses_the_preferred_scale() {
        let s = parse_state(STATE).unwrap();
        assert_eq!(
            plan(&s, "0x00000001", 0),
            None,
            "already at the preferred 2.5"
        );
        let mut custom = s.clone();
        custom.logical[1].scale = 1.0;
        let p = plan(&custom, "0x00000001", 0).unwrap();
        assert_eq!(p[1].scale, 2.5);
    }

    /// An unknown display is left alone.
    #[test]
    fn an_unlisted_display_gives_no_plan() {
        let s = parse_state(STATE).unwrap();
        assert_eq!(plan(&s, "0x00000009", 150), None);
    }

    /// The busctl arguments follow ApplyMonitorsConfig's signature: counts before every array, three words per physical monitor.
    #[test]
    fn busctl_arguments_have_the_signature_shape() {
        let s = parse_state(STATE).unwrap();
        let a = busctl_args(26, 1, &s.logical);
        assert_eq!(
            a[6], "uua(iiduba(ssa{sv}))a{sv}",
            "serial and method are both uint32"
        );
        let tail = &a[7..];
        assert_eq!(&tail[..3], ["26", "1", "3"]);
        // first logical monitor: x y scale transform primary count, then connector mode 0 props
        assert_eq!(
            &tail[3..12],
            [
                "0",
                "0",
                "1",
                "0",
                "true",
                "1",
                "eDP-1",
                "1920x1080@60.000",
                "0"
            ]
        );
        assert_eq!(tail.last().unwrap(), "0");
    }
}
