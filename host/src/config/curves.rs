//! The dim/undim easing curves: named presets and cubic-bezier parsing

use super::*;

/// Fade curves the tablet app knows how to animate; anything else falls back to the default.
pub const CURVES: [&str; 5] = [
    "linear",
    "ease_in",
    "ease_out",
    "ease_in_out",
    "exponential",
];

/// Highest and lowest control-point y of a custom Bezier fade (x is limited to 0..1).
pub const BEZIER_Y_MIN: f64 = -2.0;

/// Highest control-point y of a custom Bezier fade.
pub const BEZIER_Y_MAX: f64 = 3.0;

/// A fade curve the user saved in the GUI: a CSS-style cubic Bezier through (0,0) and (1,1).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct CurvePreset {
    pub name: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Parse `bezier:x1,y1,x2,y2` into clamped control points.
pub(super) fn parse_bezier(text: &str) -> Option<(f64, f64, f64, f64)> {
    let rest = text.strip_prefix("bezier:")?;
    let v: Vec<f64> = rest
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if v.len() != 4 || v.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let x = |n: f64| n.clamp(0.0, 1.0);
    let y = |n: f64| n.clamp(BEZIER_Y_MIN, BEZIER_Y_MAX);
    Some((x(v[0]), y(v[1]), x(v[2]), y(v[3])))
}

/// The wire form the tablet understands for a stored curve: a preset name, or
/// `bezier:x1,y1,x2,y2` for a custom or saved one (`preset:<name>` is looked up in `presets`).
/// Anything unusable becomes `fallback`.
pub fn resolve_curve(curve: &str, presets: &[CurvePreset], fallback: &str) -> String {
    let c = curve.trim();
    if let Some(name) = c.strip_prefix("preset:") {
        return presets
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| parse_bezier(&format!("bezier:{},{},{},{}", p.x1, p.y1, p.x2, p.y2)))
            .map(|b| format!("bezier:{},{},{},{}", b.0, b.1, b.2, b.3))
            .unwrap_or_else(|| fallback.to_string());
    }
    if let Some(b) = parse_bezier(c) {
        return format!("bezier:{},{},{},{}", b.0, b.1, b.2, b.3);
    }
    let lower = c.to_ascii_lowercase();
    if CURVES.contains(&lower.as_str()) {
        lower
    } else {
        fallback.to_string()
    }
}
