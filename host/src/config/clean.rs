//! Normalisers for free-text config values and the serde default helpers

/// `full_screen` (default), `standard_rectangle` or `wide_rectangle`; anything else falls back to the default.
pub fn clean_shape(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        s @ ("standard_rectangle" | "wide_rectangle") => s.to_string(),
        _ => "full_screen".into(),
    }
}

/// `mouse`, `tablet` or `auto` (ghost tracking: both devices follow the pen, the click goes to the
/// one the focused app wants); anything else is `mouse`.
pub fn clean_stylus_style(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "tablet" => "tablet".into(),
        "auto" => "auto".into(),
        _ => "mouse".into(),
    }
}

/// `icons` or `text` (the default) for the pen menu's button style.
pub fn clean_menu_style(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "icons" => "icons".into(),
        _ => "text".into(),
    }
}

/// Drawing programs that want the pressure-sensitive pen by default.
pub fn default_art_apps() -> Vec<String> {
    [
        "krita",
        "gimp",
        "inkscape",
        "blender",
        "xournalpp",
        "rnote",
        "mypaint",
        "aseprite",
        "pinta",
        "darktable",
        "freecad",
        "onlyoffice",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// What a pen side button may do: the mouse buttons `right` and `middle`, `none`, a key combination (`undo`, `redo`, `copy`, `paste`,
/// `cut`, `select-all`) or `menu` (open the pen menu).
pub const PEN_BUTTON_ACTIONS: [&str; 10] = [
    "right",
    "middle",
    "none",
    "undo",
    "redo",
    "copy",
    "paste",
    "cut",
    "select-all",
    "menu",
];

/// One of [`PEN_BUTTON_ACTIONS`]; anything else falls back to `default`.
pub fn clean_pen_button(v: &str, default: &str) -> String {
    let v = v.trim().to_ascii_lowercase();
    if PEN_BUTTON_ACTIONS.contains(&v.as_str()) {
        v
    } else {
        default.to_string()
    }
}

/// `clickpad` (default) or `classic`; anything else falls back to `clickpad`.
pub(super) fn clean_touchpad_style(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "classic" => "classic".into(),
        _ => "clickpad".into(),
    }
}

/// `light`, `medium` (default) or `strong`; anything else falls back to `medium`.
pub(super) fn clean_haptic_strength(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        s @ ("light" | "strong") => s.to_string(),
        _ => "medium".into(),
    }
}

/// `never` (default), `idle` or `always`; anything else falls back to `never`.
pub(super) fn clean_auto_open(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        m @ ("idle" | "always") => m.to_string(),
        _ => "never".into(),
    }
}

/// `bottom_right_zone` (default) or `two_finger_tap`; anything else falls back to the default.
pub(super) fn clean_right_click(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "two_finger_tap" => "two_finger_tap".into(),
        _ => "bottom_right_zone".into(),
    }
}

/// Serde default for boolean settings that are on unless the file says otherwise.
pub(super) fn default_true() -> bool {
    true
}

/// Serde default for the config's version field: this build's version.
pub(super) fn default_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
