//! `DeviceOptions`: the effective settings of one tablet (global default plus its override) and what they imply

use super::*;

/// The effective (global merged with the device's override) value of every live option.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceOptions {
    pub input_mode: InputMode,
    pub immersive_mode: bool,
    pub show_stats: bool,
    pub touch_input: bool,
    pub touch_on_lock_screen: bool,
    pub keep_display_alive_when_locked: bool,
    pub keep_connected_in_background: bool,
    pub blank_pen_only: bool,
    pub orientation: String,
    pub theme_mode: String,
    pub battery_saver_on_disconnect: bool,
    pub battery_saver_lock: bool,
    pub idle_dim_enabled: bool,
    pub idle_dim_timeout_secs: u32,
    /// Wire form: a preset name or `bezier:x1,y1,x2,y2`.
    pub dim_curve: String,
    pub undim_curve: String,
    pub dim_duration_ms: u32,
    pub undim_duration_ms: u32,
    pub touchpad_haptics: bool,
    pub touchpad_haptic_strength: String,
    pub touchpad_click_sound: bool,
    pub touchpad_style: String,
    /// Interface scale of the virtual display in percent, 0 for automatic. Host only (the tablet is not told).
    pub interface_scale: u32,
    pub touchpad_middle_button: bool,
    pub touchpad_classic_tap: bool,
    /// The setting "Use Native Gestures (Wayland/libinput)". Whether it is in effect also depends
    /// on the desktop: see [`DeviceOptions::native_gestures_effective`].
    pub native_gestures: bool,
    /// Touchpad sensitivity in percent (50-200).
    pub touchpad_sensitivity: u32,
    /// `bottom_right_zone` or `two_finger_tap`.
    pub right_click_method: String,
    /// `full_screen`, `standard_rectangle` or `wide_rectangle`.
    pub touchpad_shape: String,
    /// `mouse`, `tablet` or `auto` (ghost tracking).
    pub stylus_style: String,
    /// `right`, `middle` or `none`.
    pub stylus_button_primary: String,
    pub stylus_button_secondary: String,
    /// The pen's long-press menu is on. Global.
    pub pen_menu: bool,
    /// Pen menu buttons: `text` or `icons`.
    pub pen_menu_style: String,
    /// Milliseconds the pen is held still before the menu appears. Global.
    pub pen_menu_hold_ms: u32,
    /// Milliseconds a pen tap keeps the left button down (0 = release at once). Global.
    pub pen_click_hold_ms: u32,
    /// Ghost tracking: apps that get the pressure pen. Global.
    pub stylus_art_apps: Vec<String>,
    /// Informational: a Wi-Fi address is configured for this device, so the app hides its
    /// "Setup Wireless Pairing" button.
    pub wifi_configured: bool,
}

impl DeviceOptions {
    /// Merge `profile`'s overrides over the global values in `cfg`, clamping what a hand-edited
    /// profile could get wrong.
    pub fn resolve(cfg: &FileConfig, profile: Option<&TabletProfile>) -> Self {
        let default = TabletProfile::default();
        let p = profile.unwrap_or(&default);
        macro_rules! pick {
            ($f:ident) => {
                p.$f.clone().unwrap_or_else(|| cfg.$f.clone())
            };
        }
        let mut g = cfg.clone();
        g.immersive_mode = pick!(immersive_mode);
        g.show_stats = pick!(show_stats);
        g.touch_input = pick!(touch_input);
        g.touch_on_lock_screen = pick!(touch_on_lock_screen);
        g.keep_display_alive_when_locked = pick!(keep_display_alive_when_locked);
        g.keep_connected_in_background = pick!(keep_connected_in_background);
        g.blank_pen_only = pick!(blank_pen_only);
        g.orientation = pick!(orientation);
        g.theme_mode = pick!(theme_mode);
        g.battery_saver_on_disconnect = pick!(battery_saver_on_disconnect);
        g.battery_saver_lock = pick!(battery_saver_lock);
        g.idle_dim_enabled = pick!(idle_dim_enabled);
        g.idle_dim_timeout_secs = pick!(idle_dim_timeout_secs);
        g.dim_curve = pick!(dim_curve);
        g.undim_curve = pick!(undim_curve);
        g.dim_duration_ms = pick!(dim_duration_ms);
        g.undim_duration_ms = pick!(undim_duration_ms);
        g.touchpad_haptics = pick!(touchpad_haptics);
        g.touchpad_haptic_strength = pick!(touchpad_haptic_strength);
        g.touchpad_click_sound = pick!(touchpad_click_sound);
        g.touchpad_style = pick!(touchpad_style);
        g.interface_scale = pick!(interface_scale);
        g.touchpad_middle_button = pick!(touchpad_middle_button);
        g.touchpad_classic_tap = pick!(touchpad_classic_tap);
        g.native_gestures = p.native_gestures.unwrap_or(cfg.native_gestures);
        g.touchpad_sensitivity = pick!(touchpad_sensitivity);
        g.right_click_method = pick!(right_click_method);
        g.touchpad_shape = pick!(touchpad_shape);
        g.stylus_style = pick!(stylus_style);
        g.stylus_button_primary = pick!(stylus_button_primary);
        g.stylus_button_secondary = pick!(stylus_button_secondary);
        g.pen_menu = pick!(pen_menu);
        g.pen_menu_style = pick!(pen_menu_style);
        g.pen_menu_hold_ms = pick!(pen_menu_hold_ms);
        g.sanitize();
        let global_mode = CLI_MODE
            .get()
            .copied()
            .or(cfg.input_mode)
            .unwrap_or_default();
        let wifi = |s: &Option<String>| s.as_deref().is_some_and(|a| !a.trim().is_empty());
        Self {
            input_mode: p.input_mode.unwrap_or(global_mode),
            immersive_mode: g.immersive_mode,
            show_stats: g.show_stats,
            touch_input: g.touch_input,
            touch_on_lock_screen: g.touch_on_lock_screen,
            keep_display_alive_when_locked: g.keep_display_alive_when_locked,
            keep_connected_in_background: g.keep_connected_in_background,
            blank_pen_only: g.blank_pen_only,
            orientation: g.orientation.clone(),
            theme_mode: g.theme_mode.clone(),
            battery_saver_on_disconnect: g.battery_saver_on_disconnect,
            battery_saver_lock: g.battery_saver_lock,
            idle_dim_enabled: g.idle_dim_enabled,
            idle_dim_timeout_secs: g.idle_dim_timeout_secs,
            dim_curve: resolve_curve(&g.dim_curve, &cfg.curve_presets, "ease_out"),
            undim_curve: resolve_curve(&g.undim_curve, &cfg.curve_presets, "ease_in"),
            dim_duration_ms: g.dim_duration_ms,
            undim_duration_ms: g.undim_duration_ms,
            touchpad_haptics: g.touchpad_haptics,
            touchpad_haptic_strength: g.touchpad_haptic_strength.clone(),
            touchpad_click_sound: g.touchpad_click_sound,
            touchpad_style: g.touchpad_style.clone(),
            interface_scale: g.interface_scale,
            touchpad_middle_button: g.touchpad_middle_button,
            touchpad_classic_tap: g.touchpad_classic_tap,
            native_gestures: g.native_gestures,
            touchpad_sensitivity: g.touchpad_sensitivity,
            right_click_method: g.right_click_method.clone(),
            touchpad_shape: g.touchpad_shape.clone(),
            stylus_style: g.stylus_style.clone(),
            stylus_button_primary: g.stylus_button_primary.clone(),
            stylus_button_secondary: g.stylus_button_secondary.clone(),
            pen_menu: g.pen_menu,
            pen_menu_style: g.pen_menu_style.clone(),
            pen_menu_hold_ms: g.pen_menu_hold_ms,
            pen_click_hold_ms: g.pen_click_hold_ms,
            stylus_art_apps: g.stylus_art_apps.clone(),
            wifi_configured: wifi(&p.wifi_address) || !cfg.wifi_address.trim().is_empty(),
        }
    }

    /// The `config_update` message the tablet applies to its own preferences and UI.
    pub fn config_update(&self) -> Value {
        json!({
            "type": "config_update",
            "input_mode": self.input_mode.as_str(),
            "immersive_mode": self.immersive_mode,
            "show_stats": self.show_stats,
            "touch_input": self.touch_input,
            "touch_on_lock_screen": self.touch_on_lock_screen,
            "keep_display_alive_when_locked": self.keep_display_alive_when_locked,
            "keep_connected_in_background": self.keep_connected_in_background,
            "blank_pen_only": self.blank_pen_only,
            "orientation": self.orientation,
            "theme_mode": self.theme_mode,
            "battery_saver_on_disconnect": self.battery_saver_on_disconnect,
            "battery_saver_lock": self.battery_saver_lock,
            "idle_dim_enabled": self.idle_dim_enabled,
            "idle_dim_timeout_secs": self.idle_dim_timeout_secs,
            "dim_curve": self.dim_curve,
            "undim_curve": self.undim_curve,
            "dim_duration_ms": self.dim_duration_ms,
            "undim_duration_ms": self.undim_duration_ms,
            "touchpad_haptics": self.touchpad_haptics,
            "touchpad_haptic_strength": self.touchpad_haptic_strength,
            "touchpad_click_sound": self.touchpad_click_sound,
            "touchpad_style": self.touchpad_style,
            "touchpad_middle_button": self.touchpad_middle_button,
            "touchpad_classic_tap": self.touchpad_classic_tap,
            "right_click_method": self.right_click_method,
            // What the tablet acts on is the effective value, so it falls back to its own gesture
            // recogniser on X11 and on desktops that do nothing with libinput gestures.
            "native_gestures": self.native_gestures_effective(),
            "touchpad_sensitivity": self.touchpad_sensitivity,
            "touchpad_shape": self.touchpad_shape,
            "stylus_style": self.stylus_style,
            "stylus_button_primary": self.stylus_button_primary,
            "stylus_button_secondary": self.stylus_button_secondary,
            "pen_menu": self.pen_menu,
            "pen_menu_style": self.pen_menu_style,
            "pen_menu_hold_ms": self.pen_menu_hold_ms,
            "wifi_configured": self.wifi_configured,
        })
    }

    // PLATFORM(wayland): reads XDG_SESSION_TYPE and XDG_CURRENT_DESKTOP to decide whether the tablet may send raw touchpad frames
    /// Whether touchpad mode really uses the virtual multi-touch touchpad: the setting is on and
    /// the desktop turns libinput's swipe and pinch events into something (GNOME and KDE on
    /// Wayland). On X11, Cinnamon, wlroots compositors and the like the tablet recognises the
    /// gestures itself and the host sends keys, as it did before.
    pub fn native_gestures_effective(&self) -> bool {
        self.native_gestures
            && native_gestures_supported(
                &std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
                &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            )
    }

    /// The message that tells the tablet app to dim its screen, with the configured duration and
    /// curve.
    pub(super) fn dim_message(&self) -> Value {
        json!({ "type": "dim_screen", "duration": self.dim_duration_ms, "curve": self.dim_curve })
    }

    /// The message that tells the tablet app to undim its screen, with the configured duration and
    /// curve.
    pub(super) fn undim_message(&self) -> Value {
        json!({ "type": "undim_screen", "duration": self.undim_duration_ms, "curve": self.undim_curve })
    }
}

// PLATFORM(wayland): libinput gestures reach apps only on a Wayland session of GNOME or KDE; X11 and other desktops get host-side gestures
/// Whether this session's compositor consumes libinput swipe / pinch events itself: GNOME (Mutter)
/// and KDE (KWin), both on Wayland. `session` is `XDG_SESSION_TYPE`, `desktop` is
/// `XDG_CURRENT_DESKTOP` (a colon-separated list such as `ubuntu:GNOME`).
pub fn native_gestures_supported(session: &str, desktop: &str) -> bool {
    if !session.eq_ignore_ascii_case("wayland") {
        return false;
    }
    desktop
        .split(':')
        .any(|d| matches!(d.trim().to_ascii_uppercase().as_str(), "GNOME" | "KDE"))
}
