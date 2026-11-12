//! The display capability: bringing the virtual (EVDI) output up and down on the user's desktop, with one backend per desktop.
//!
//! Each backend knows how its compositor enables, places and removes an output; this file only picks the backend and
//! passes the connector names of the right EVDI card to it. The same shape as `scale.rs`, and the model for new capabilities.
//!
//! | Desktop | Enable and place | Disable |
//! |---|---|---|
//! | Hyprland | `hyprctl` (`hyprland::display`) | `hyprctl` |
//! | COSMIC | `cosmic-randr` (`cosmic::display`), no placement | `cosmic-randr` |
//! | GNOME (Wayland) | nothing: Mutter switches a new monitor on and places it itself | nothing |
//! | X11 (not Plasma) | `xrandr` (`x11::display`) | `xrandr` |
//! | KDE Plasma, and every desktop not listed | `kscreen-doctor` (`kde::display`) | `kscreen-doctor` |
//!
//! To add a desktop: write `desktop/<name>/display.rs` with `pub fn active() -> bool`, `enable_output(names, position)` and
//! `disable_output(names)`, add a variant to [`DisplayBackend`], and add it to [`backend`], [`enable_output`] and
//! [`disable_output`] below. Input mapping for the desktop is in `input/mapping.rs` and uses [`backend`] too.

use tracing::warn;

/// A desktop whose output handling UScreen knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayBackend {
    /// `hyprctl`.
    Hyprland,
    /// `cosmic-randr`.
    Cosmic,
    /// Mutter on Wayland: outputs and input mapping through D-Bus and gsettings.
    Gnome,
    /// An X11 session other than Plasma: `xrandr` and `xinput`.
    X11,
    /// KDE Plasma, and the fall-through for any desktop none of the others recognise (Sway, ...): `kscreen-doctor` and KWin D-Bus.
    /// On a desktop that is not Plasma those tools are missing, every call finds nothing, and the feature does nothing but log.
    Kde,
}

// PLATFORM(linux): the one place that decides the desktop for display work; the order is Hyprland, COSMIC, GNOME, X11, then the KDE fall-through
/// The backend for this session. Checked in the order of the table above; the first whose `active()` says yes wins.
pub fn backend() -> DisplayBackend {
    if super::hyprland::display::active() {
        DisplayBackend::Hyprland
    } else if super::cosmic::display::active() {
        DisplayBackend::Cosmic
    } else if super::gnome::display::active() {
        DisplayBackend::Gnome
    } else if super::x11::display::active() {
        DisplayBackend::X11
    } else {
        DisplayBackend::Kde
    }
}

/// The DRM connector names of the virtual display of `card` (of every EVDI card when `card` is `None`, as before the helper reported it).
///
/// Read from sysfs rather than guessed from the name: a real DVI monitor on a dock has the same name pattern, and acting on
/// it would enable and move the user's physical screen.
pub fn connector_names(card: Option<u32>) -> Vec<String> {
    crate::display::vdisplay::evdi_connectors()
        .into_iter()
        .filter(|c| card.is_none_or(|want| c.card == want))
        .map(|c| c.name)
        .collect()
}

/// Enable the virtual output of `card` and place it on the `position` side of the other screens.
///
/// Returns when the desktop has been asked; a backend that cannot do it logs a warning and the output stays as the desktop made it.
/// Without a connector in sysfs there is nothing to enable: a warning is logged and nothing else happens.
pub async fn enable_output(card: Option<u32>, position: crate::config::Position) {
    let names = connector_names(card);
    if names.is_empty() {
        warn!("No EVDI connector found in sysfs — cannot enable the virtual display");
        return;
    }
    match backend() {
        DisplayBackend::Hyprland => super::hyprland::display::enable_output(&names, position).await,
        DisplayBackend::Cosmic => super::cosmic::display::enable_output(&names).await,
        // GNOME places and switches on a new monitor by itself.
        DisplayBackend::Gnome => {}
        DisplayBackend::X11 => super::x11::display::enable_output(&names, position).await,
        DisplayBackend::Kde => super::kde::display::enable_output(&names, position).await,
    }
}

/// Switch the virtual output of `card` off so its windows return to the real screens. Silent when it has no connector.
pub async fn disable_output(card: Option<u32>) {
    let names = connector_names(card);
    if names.is_empty() {
        return;
    }
    match backend() {
        DisplayBackend::Hyprland => super::hyprland::display::disable_output(&names).await,
        DisplayBackend::Cosmic => super::cosmic::display::disable_output(&names).await,
        DisplayBackend::Gnome => {}
        DisplayBackend::X11 => super::x11::display::disable_output(&names).await,
        DisplayBackend::Kde => super::kde::display::disable_output(&names).await,
    }
}
