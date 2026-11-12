//! The interface scale of a UScreen virtual display, with one small backend per desktop.
//!
//! Each backend knows how its compositor sets a monitor's scale and how to leave it alone when it already is right; this file only chooses
//! the backend and keeps the scale in place:
//!
//! | Desktop | How | Status |
//! |---|---|---|
//! | GNOME (Wayland) | Mutter `ApplyMonitorsConfig`, temporary layout | tested on GNOME 46 |
//! | Hyprland | `hyprctl keyword monitor name,WxH@Hz,XxY,scale` | command building tested, not run on Hyprland |
//! | KDE Plasma | `kscreen-doctor output.NAME.scale.S` | command building tested, not run on Plasma |
//! | X11 | none: X11 has no per-output interface scale | the setting does nothing |
//!
//! To add a desktop: write `desktop/<name>/scale.rs` with `async fn apply(connectors: &[String], percent: u32) -> bool` (0 = automatic;
//! cheap and harmless when nothing needs to change), add it to [`Backend`] and to [`backend`] and [`apply`] below.

use std::time::Duration;
use tokio::sync::watch;

/// A desktop with a way to set the scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Mutter's `DisplayConfig` on GNOME Wayland.
    Gnome,
    /// `hyprctl keyword monitor`.
    Hyprland,
    /// `kscreen-doctor`.
    Kde,
    /// An X11 session: recognised, but there is nothing to set.
    X11,
}

/// Which display to change: the slot of the tablet and the EVDI card that slot owns, when known.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    /// Slot of the tablet (0 for the first).
    pub instance: u32,
    /// The EVDI card of its virtual display, once the helper reported it.
    pub card: Option<u32>,
}

/// The backend for this session, or `None` when the desktop is unknown. The order is the one the input mapping uses.
pub fn backend() -> Option<Backend> {
    if crate::desktop::hyprland::display::active() {
        Some(Backend::Hyprland)
    } else if crate::desktop::gnome::display::active() {
        Some(Backend::Gnome)
    } else if crate::desktop::x11::display::active() {
        Some(Backend::X11)
    } else if std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| std::path::Path::new(d).join("kscreen-doctor").exists())
    {
        Some(Backend::Kde)
    } else {
        None
    }
}

/// Whether the scale can be set on this desktop at all.
pub fn supported() -> bool {
    matches!(
        backend(),
        Some(Backend::Gnome | Backend::Hyprland | Backend::Kde)
    )
}

/// The connector names of the virtual displays of [target]'s card (all of them when the card is not known yet).
fn connectors(target: &Target) -> Vec<String> {
    crate::display::vdisplay::evdi_connectors()
        .into_iter()
        .filter(|c| target.card.is_none_or(|want| c.card == want))
        .map(|c| c.name)
        .collect()
}

/// Set the interface scale of the display of [target] to [percent] (0 = automatic). True when the display is as asked afterwards, also
/// when it already was.
pub async fn apply(target: &Target, percent: u32) -> bool {
    match backend() {
        Some(Backend::Gnome) => crate::desktop::gnome::scale::apply(target.instance, percent).await,
        Some(Backend::Hyprland) => {
            crate::desktop::hyprland::scale::apply(&connectors(target), percent).await
        }
        Some(Backend::Kde) => crate::desktop::kde::scale::apply(&connectors(target), percent).await,
        Some(Backend::X11) | None => false,
    }
}

/// Keep a custom scale in place for the life of the daemon: every few seconds, while a custom percentage is set and the tablet is a screen,
/// apply it again. The desktop forgets a temporary scale when it reconfigures its monitors (a real monitor plugged in, a change in its
/// display settings), and this puts it back without waiting for the next display restart. Applying is a no-op when nothing differs.
pub async fn keep(
    instance: u32,
    opts: watch::Receiver<crate::config::options::DeviceOptions>,
    card: watch::Receiver<Option<u32>>,
    pen_only: watch::Receiver<bool>,
) {
    if !supported() {
        return;
    }
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let percent = opts.borrow().interface_scale;
        let screen = !*pen_only.borrow();
        let card = *card.borrow();
        if percent > 0 && screen {
            apply(&Target { instance, card }, percent).await;
        }
    }
}
