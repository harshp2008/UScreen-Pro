//! The desktop layer: everything the host needs to know or do that depends on *which desktop* the
//! user runs, behind one interface ([`DesktopBackend`]) with one backend per environment.
//!
//! The rest of the host (input routing, the pen menu) never asks "is this GNOME?"; it asks the
//! backend what it can do ([`Capabilities`]) and degrades when the answer is "nothing":
//!
//! * `gnome` - GNOME Shell on Wayland, through the `uscreen-app-lock` Shell extension.
//! * `x11` - any X11 session (`_NET_ACTIVE_WINDOW`).
//! * `generic` - an unknown desktop: reads the same focus file a helper script may write, otherwise
//!   knows nothing. Every feature that needs the desktop is simply unavailable there.
//!
//! To support a new desktop add one file here that implements [`DesktopBackend`], and pick it in
//! [`detect`]. See `docs/desktop-backends.md`.

mod focus_file;
mod generic;
mod gnome;
mod x11;

use serde::Deserialize;

/// What a backend can do. Features check this instead of the desktop's name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Reports which app has focus (lets the pen pick the art route for Krita, GIMP, ...).
    pub focus: bool,
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

/// One desktop environment's answers to the questions the host asks.
pub trait DesktopBackend: Send {
    /// Short name for logs (`"gnome"`, `"x11"`, `"generic"`).
    fn name(&self) -> &'static str;
    /// What this backend supports.
    fn capabilities(&self) -> Capabilities;
    /// Which app has focus now. Called a few times a second from one thread.
    fn poll_focus(&mut self) -> FocusUpdate;
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
