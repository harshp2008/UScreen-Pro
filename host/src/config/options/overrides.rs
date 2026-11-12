//! Applying one setting changed on the tablet to its profile in config.toml

use super::*;
use crate::config::clean::PEN_BUTTON_ACTIONS;

/// Store one setting pushed by the tablet as the device's override. `Ok(true)` when it changed.
pub(super) fn set_override(p: &mut TabletProfile, key: &str, v: &Value) -> Result<bool, ()> {
    /// Store `new` in `slot`; true if that changed it.
    fn set<T: PartialEq>(slot: &mut Option<T>, new: T) -> bool {
        if slot.as_ref() == Some(&new) {
            false
        } else {
            *slot = Some(new);
            true
        }
    }
    let b = v.as_bool().ok_or(());
    let n = v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or(());
    let s = v.as_str().map(str::to_string).ok_or(());
    Ok(match key {
        "input_mode" => {
            let mode = match s?.as_str() {
                "display" => InputMode::Display,
                "touchpad" => InputMode::Touchpad,
                "stylus" => InputMode::Stylus,
                _ => return Err(()),
            };
            set(&mut p.input_mode, mode)
        }
        "immersive_mode" => set(&mut p.immersive_mode, b?),
        "show_stats" => set(&mut p.show_stats, b?),
        "touch_input" => set(&mut p.touch_input, b?),
        "touch_on_lock_screen" => set(&mut p.touch_on_lock_screen, b?),
        "keep_display_alive_when_locked" => set(&mut p.keep_display_alive_when_locked, b?),
        "keep_connected_in_background" => set(&mut p.keep_connected_in_background, b?),
        "blank_pen_only" => set(&mut p.blank_pen_only, b?),
        "orientation" => set(&mut p.orientation, s?),
        "theme_mode" => set(&mut p.theme_mode, s?),
        "battery_saver_on_disconnect" => set(&mut p.battery_saver_on_disconnect, b?),
        "battery_saver_lock" => set(&mut p.battery_saver_lock, b?),
        "idle_dim_enabled" => set(&mut p.idle_dim_enabled, b?),
        "idle_dim_timeout_secs" => set(&mut p.idle_dim_timeout_secs, n?),
        "touchpad_haptics" => set(&mut p.touchpad_haptics, b?),
        "touchpad_haptic_strength" => match s?.as_str() {
            m @ ("light" | "medium" | "strong") => {
                set(&mut p.touchpad_haptic_strength, m.to_string())
            }
            _ => return Err(()),
        },
        "touchpad_click_sound" => set(&mut p.touchpad_click_sound, b?),
        "touchpad_style" => match s?.as_str() {
            m @ ("clickpad" | "classic") => set(&mut p.touchpad_style, m.to_string()),
            _ => return Err(()),
        },
        "touchpad_middle_button" => set(&mut p.touchpad_middle_button, b?),
        "touchpad_classic_tap" => set(&mut p.touchpad_classic_tap, b?),
        "native_gestures" => set(&mut p.native_gestures, b?),
        "touchpad_sensitivity" => set(&mut p.touchpad_sensitivity, n?.clamp(50, 200)),
        "touchpad_shape" => match s?.as_str() {
            m @ ("full_screen" | "standard_rectangle" | "wide_rectangle") => {
                set(&mut p.touchpad_shape, m.to_string())
            }
            _ => return Err(()),
        },
        "stylus_style" => match s?.as_str() {
            m @ ("mouse" | "tablet" | "auto") => set(&mut p.stylus_style, m.to_string()),
            _ => return Err(()),
        },
        "stylus_button_primary" => match s?.as_str() {
            m if PEN_BUTTON_ACTIONS.contains(&m) => {
                set(&mut p.stylus_button_primary, m.to_string())
            }
            _ => return Err(()),
        },
        "pen_menu" => set(&mut p.pen_menu, b?),
        "pen_menu_style" => match s?.as_str() {
            m @ ("text" | "icons") => set(&mut p.pen_menu_style, m.to_string()),
            _ => return Err(()),
        },
        "pen_menu_hold_ms" => set(&mut p.pen_menu_hold_ms, n?.clamp(500, 1500)),
        "stylus_button_secondary" => match s?.as_str() {
            m if PEN_BUTTON_ACTIONS.contains(&m) => {
                set(&mut p.stylus_button_secondary, m.to_string())
            }
            _ => return Err(()),
        },
        "right_click_method" => match s?.as_str() {
            m @ ("bottom_right_zone" | "two_finger_tap") => {
                set(&mut p.right_click_method, m.to_string())
            }
            _ => return Err(()),
        },
        _ => return Err(()),
    })
}
