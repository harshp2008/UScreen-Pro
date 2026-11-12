//! The interface scale of a virtual display on KDE Plasma, through `kscreen-doctor`.

/// The `kscreen-doctor` argument that gives [connector] [percent], read from `kscreen-doctor -j`; `None` when the output is not listed or
/// already has that scale.
pub fn argument(kscreen_json: &str, connector: &str, percent: u32) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(kscreen_json).ok()?;
    let o = v
        .get("outputs")?
        .as_array()?
        .iter()
        .find(|o| o.get("name").and_then(|n| n.as_str()) == Some(connector))?;
    let wanted = percent as f64 / 100.0;
    if (o.get("scale")?.as_f64()? - wanted).abs() < 0.005 {
        return None;
    }
    Some(format!("output.{connector}.scale.{wanted:.2}"))
}

/// Set the scale of the first of [connectors] KDE lists. Automatic (0) leaves it as it is: Plasma has no "automatic" to go back to.
pub async fn apply(connectors: &[String], percent: u32) -> bool {
    if percent == 0 {
        return true;
    }
    let Ok(out) = tokio::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .await
    else {
        return false;
    };
    let json = String::from_utf8_lossy(&out.stdout);
    for c in connectors {
        if let Some(arg) = argument(&json, c, percent) {
            return tokio::process::Command::new("kscreen-doctor")
                .arg(arg)
                .status()
                .await
                .is_ok_and(|s| s.success());
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUTS: &str =
        r#"{"outputs":[{"name":"eDP-1","scale":1.0},{"name":"DVI-I-1","scale":1.0}]}"#;

    /// The argument names the output and the scale as a decimal.
    #[test]
    fn the_argument_names_the_output_and_scale() {
        assert_eq!(
            argument(OUTPUTS, "DVI-I-1", 150).unwrap(),
            "output.DVI-I-1.scale.1.50"
        );
    }

    /// Nothing to do when the scale already matches or the output is missing.
    #[test]
    fn no_argument_when_nothing_changes() {
        assert_eq!(argument(OUTPUTS, "DVI-I-1", 100), None);
        assert_eq!(argument(OUTPUTS, "DP-9", 150), None);
    }
}
