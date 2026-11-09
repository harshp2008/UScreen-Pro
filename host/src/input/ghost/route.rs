//! Which app the pen is drawing into, and the route that follows from it: `true` sends the tip to
//! the pressure-sensitive tablet device (art apps, or a hotkey override), `false` makes it a mouse
//! click. Holds the last focus the desktop backend reported and the user's per-app overrides.

use crate::desktop::AppIdentity;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct FocusState {
    app: Option<AppIdentity>,
    /// Per-app route chosen with the hotkey: `true` = tablet (pen), `false` = mouse.
    overrides: HashMap<String, bool>,
    /// Override used while the focused app is unknown (no extension, KDE Wayland).
    unknown_override: Option<bool>,
}

fn focus() -> &'static Mutex<FocusState> {
    static F: OnceLock<Mutex<FocusState>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(FocusState::default()))
}

/// Whether the pen's tip should go to the pressure-sensitive tablet device for the focused app.
pub(in crate::input) fn route_to_pen(art_apps: &[String]) -> bool {
    let f = focus().lock().unwrap_or_else(|e| e.into_inner());
    decide(f.app.as_ref(), &f.overrides, f.unknown_override, art_apps)
}

pub(super) fn decide(
    app: Option<&AppIdentity>,
    overrides: &HashMap<String, bool>,
    unknown_override: Option<bool>,
    art_apps: &[String],
) -> bool {
    match app.and_then(|a| a.key().map(|k| (a, k))) {
        Some((a, key)) => overrides
            .get(&key)
            .copied()
            .unwrap_or_else(|| a.is_art(art_apps)),
        None => unknown_override.unwrap_or(false),
    }
}

/// Ctrl+Alt+P: flip the route for the focused app (or for "unknown" when no focus source exists).
/// Returns the new route (`true` = tablet).
pub(in crate::input) fn toggle_route(art_apps: &[String]) -> bool {
    let mut f = focus().lock().unwrap_or_else(|e| e.into_inner());
    let now = decide(f.app.as_ref(), &f.overrides, f.unknown_override, art_apps);
    let key = f.app.as_ref().and_then(|a| a.key());
    match key {
        Some(k) => {
            f.overrides.insert(k, !now);
        }
        None => f.unknown_override = Some(!now),
    }
    !now
}

pub(in crate::input) fn set_focus(app: Option<AppIdentity>) {
    focus().lock().unwrap_or_else(|e| e.into_inner()).app = app;
}
