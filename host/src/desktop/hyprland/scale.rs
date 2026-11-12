//! The interface scale of a virtual display on Hyprland, through `hyprctl keyword monitor`.

use super::display::{hyprctl, keyword};

/// The monitor rule that gives `connector` `percent` (0 = `auto`) and keeps everything else about it, read from `hyprctl -j monitors`;
/// `None` when the monitor is not listed or already has that scale.
pub fn rule(monitors_json: &str, connector: &str, percent: u32) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(monitors_json).ok()?;
    let m = v
        .as_array()?
        .iter()
        .find(|m| m.get("name").and_then(|n| n.as_str()) == Some(connector))?;
    let (w, h) = (m.get("width")?.as_u64()?, m.get("height")?.as_u64()?);
    let hz = m.get("refreshRate")?.as_f64()?;
    let (x, y) = (m.get("x")?.as_i64()?, m.get("y")?.as_i64()?);
    let current = m.get("scale")?.as_f64()?;
    let scale = if percent == 0 {
        "auto".to_string()
    } else {
        let wanted = percent as f64 / 100.0;
        if (current - wanted).abs() < 0.005 {
            return None;
        }
        format!("{wanted:.2}")
    };
    Some(format!("{connector},{w}x{h}@{hz:.2},{x}x{y},{scale}"))
}

/// Set the scale of the first of `connectors` Hyprland lists. True when it is as asked afterwards.
pub async fn apply(connectors: &[String], percent: u32) -> bool {
    let Some(json) = hyprctl(&["-j", "monitors"]).await else {
        return false;
    };
    // "auto" is applied only after a custom value, which this daemon cannot tell from Hyprland's own choice: leave it to Hyprland.
    if percent == 0 {
        return true;
    }
    for c in connectors {
        if let Some(rule) = rule(&json, c, percent) {
            return keyword("monitor", &rule).await;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `hyprctl monitors -j` output with the laptop panel and one virtual monitor.
    const MONITORS: &str = r#"[{"name":"eDP-1","width":1920,"height":1080,"refreshRate":60.0,"x":0,"y":0,"scale":1.0},
        {"name":"DVI-I-1","width":2560,"height":1600,"refreshRate":89.999,"x":1920,"y":0,"scale":1.0}]"#;

    /// The rule keeps the mode and position and changes only the scale.
    #[test]
    fn the_rule_changes_only_the_scale() {
        assert_eq!(
            rule(MONITORS, "DVI-I-1", 175).unwrap(),
            "DVI-I-1,2560x1600@90.00,1920x0,1.75"
        );
        assert_eq!(
            rule(MONITORS, "DVI-I-1", 0).unwrap(),
            "DVI-I-1,2560x1600@90.00,1920x0,auto"
        );
    }

    /// Nothing to do when the scale already matches, or the monitor is not there.
    #[test]
    fn no_rule_when_nothing_changes() {
        assert_eq!(rule(MONITORS, "DVI-I-1", 100), None);
        assert_eq!(rule(MONITORS, "HDMI-A-1", 150), None);
        assert_eq!(rule("nonsense", "DVI-I-1", 150), None);
    }
}
