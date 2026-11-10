//! The desktop layer: everything the host needs to know or do that depends on *which desktop* the
//! user runs, behind one interface ([`DesktopBackend`]) with one backend per environment.
//!
//! The rest of the host (input routing, the pen menu) never asks "is this GNOME?"; it asks the
//! backend what it can do ([`Capabilities`]) and degrades when the answer is "nothing":
//!
//! One directory per desktop; each holds everything that desktop needs (`mod.rs`: the
//! [`DesktopBackend`]; `display.rs`: enabling, placing and mapping the virtual monitor; helpers):
//!
//! * `gnome` - GNOME Shell on Wayland, through the `uscreen-app-lock` Shell extension.
//! * `x11` - any X11 session (`_NET_ACTIVE_WINDOW`, RandR).
//! * `kde` - KDE Plasma: virtual monitor through KWin and the on-screen keyboard policy. No backend
//!   yet, so the pen menu and focus routing fall back to `generic`.
//! * `hyprland`, `cosmic` - virtual monitor placement only, for the same reason.
//! * `generic` - an unknown desktop: reads the same focus file a helper script may write, otherwise
//!   knows nothing. Every feature that needs the desktop is simply unavailable there.
//!
//! To support a new desktop add one directory here whose `mod.rs` implements [`DesktopBackend`], and
//! pick it in [`detect`]. See `docs/adding-support.md` and `docs/desktop-backends.md`.

mod context;
pub mod cosmic;
mod focus_file;
mod generic;
pub mod gnome;
pub mod hyprland;
pub mod kde;
pub mod x11;

#[cfg(test)]
pub use context::Rect;
pub use context::{PointerContext, Zone};
pub use gnome::spawn_at::WindowRef;
use std::time::Duration;

use serde::Deserialize;

/// What a backend can do. Features check this instead of the desktop's name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Reports which app has focus (lets the pen pick the art route for Krita, GIMP, ...).
    pub focus: bool,
    /// Can describe the spot under the pointer: surface kind, window frame, selection/clipboard size.
    pub context: bool,
    /// Can show a window exactly at the pointer and hand focus back to the previous window.
    pub placement: bool,
    /// Can draw the pen menu itself, inside the desktop shell: no window, nothing in the taskbar,
    /// no focus change (preferred over `placement` when available).
    pub menu: bool,
}

/// What a focus poll found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusUpdate {
    /// Nothing changed since the last poll.
    Unchanged,
    /// This app has focus now.
    Known(AppIdentity),
    /// No app has focus, or the desktop cannot say.
    Unknown,
}

/// A pen menu to draw: which buttons, in which style, and how long it may stay up unanswered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRequest {
    /// Matches the answer to the request.
    pub id: u64,
    /// Button names in display order (`cut`, `copy`, `paste`, `select-all`, `undo`).
    pub items: Vec<String>,
    /// `text` or `icons`.
    pub style: String,
    /// The menu closes itself after this long.
    pub timeout: Duration,
}

/// One desktop environment's answers to the questions the host asks.
pub trait DesktopBackend: Send {
    /// Short name for logs (`"gnome"`, `"x11"`, `"generic"`).
    fn name(&self) -> &'static str;
    /// What this backend supports.
    fn capabilities(&self) -> Capabilities;
    /// Which app has focus now. Called a few times a second from one thread.
    fn poll_focus(&mut self) -> FocusUpdate;

    /// Describe the spot under the pointer, waiting at most `timeout` for the desktop to answer.
    /// `None` when unsupported ([`Capabilities::context`]) or the desktop did not answer in time.
    fn context_at_pointer(&mut self, _timeout: Duration) -> Option<PointerContext> {
        None
    }

    /// The window that has focus now, so it can be given focus back ([`Self::focus_window`]).
    fn focused_window(&mut self) -> Option<WindowRef> {
        None
    }

    /// Give focus to `window`. True on success.
    fn focus_window(&mut self, _window: &WindowRef) -> bool {
        false
    }

    /// Draw the pen menu at the pointer, inside the desktop shell. True once it is on screen; false
    /// when unsupported ([`Capabilities::menu`]) or the shell did not confirm in time.
    fn show_menu(&mut self, _req: &MenuRequest) -> bool {
        false
    }

    /// Wait for the choice made in the menu shown for `req`: the button's name, `None` if it was
    /// dismissed or timed out.
    ///
    /// `close_now` is polled while waiting; once it returns true the menu is asked to close (the
    /// answer then comes back as a dismissal). Used when a tap elsewhere should dismiss the menu.
    fn wait_menu_choice(
        &mut self,
        _req: &MenuRequest,
        _close_now: &dyn Fn() -> bool,
    ) -> Option<String> {
        None
    }

    /// Start `command` with its window centred on the pointer, `lift` pixels above it; returns once
    /// the window is placed (the program keeps running). `class` is the window's app id / class,
    /// which some backends need to find the window. True on success. Supported when
    /// [`Capabilities::placement`] is set.
    fn spawn_at_pointer(&mut self, _lift: i32, _class: &str, _command: &[String]) -> bool {
        false
    }
}

/// Pick the backend for this session: X11 if it is one and connects, GNOME if the desktop says
/// so, otherwise the generic one.
pub fn detect() -> Box<dyn DesktopBackend> {
    if session_is_x11() {
        if let Some(b) = x11::X11Desktop::connect() {
            return Box::new(b);
        }
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if desktop.contains("gnome") {
        Box::new(gnome::GnomeDesktop::new())
    } else {
        Box::new(generic::GenericDesktop::new())
    }
}

/// Whether the session is X11, where ghost tracking is unnecessary (one core pointer).
pub fn session_is_x11() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("x11"))
}

/// `~/.local/share/uscreen/<name>`: where the Shell extension, the CLI and the daemon exchange small
/// files (focus, route toggle, recorder requests).
pub fn data_file(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::Path::new(&home)
            .join(".local/share/uscreen")
            .join(name),
    )
}

/// Identity of the focused window, any of which may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct AppIdentity {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub wm_class: String,
    #[serde(default)]
    pub gtk_id: String,
}

impl AppIdentity {
    /// The app's non-empty names, lower-cased and without `.desktop`.
    fn names(&self) -> impl Iterator<Item = String> + '_ {
        [&self.app_id, &self.wm_class, &self.gtk_id]
            .into_iter()
            .map(|s| s.trim().trim_end_matches(".desktop").to_ascii_lowercase())
            .filter(|s| !s.is_empty())
    }

    /// Stable key for per-app overrides: the first non-empty name.
    pub fn key(&self) -> Option<String> {
        self.names().next()
    }

    /// Whether any of this app's names contains an entry of `art_apps` (lowercase).
    pub fn is_art(&self, art_apps: &[String]) -> bool {
        self.names().any(|n| {
            art_apps
                .iter()
                .any(|a| !a.is_empty() && n.contains(a.as_str()))
        })
    }
}
