//! X11 sessions: the focused app is the `_NET_ACTIVE_WINDOW` property of the root window and its
//! `WM_CLASS`, read over x11rb.

use super::{AppIdentity, Capabilities, DesktopBackend, FocusUpdate};

/// `_NET_ACTIVE_WINDOW` and `WM_CLASS` over x11rb, for X11 sessions.
pub(super) struct X11Desktop {
    conn: x11rb::rust_connection::RustConnection,
    root: u32,
    active_atom: u32,
}

impl X11Desktop {
    pub(super) fn connect() -> Option<Self> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::ConnectionExt;
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let active_atom = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok()?.reply().ok()?.atom;
        Some(Self { conn, root, active_atom })
    }

    fn active(&mut self) -> Option<AppIdentity> {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
        let win = self
            .conn
            .get_property(false, self.root, self.active_atom, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()?;
        if win == 0 {
            return None;
        }
        let reply = self
            .conn
            .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .ok()?
            .reply()
            .ok()?;
        // "instance\0class\0": both are useful names.
        let mut parts = reply.value.split(|b| *b == 0).map(|p| String::from_utf8_lossy(p).to_string());
        let instance = parts.next().unwrap_or_default();
        let class = parts.next().unwrap_or_default();
        Some(AppIdentity { app_id: instance, wm_class: class, gtk_id: String::new() })
    }
}

impl DesktopBackend for X11Desktop {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { focus: true }
    }

    fn poll_focus(&mut self) -> FocusUpdate {
        match self.active() {
            Some(app) => FocusUpdate::Known(app),
            None => FocusUpdate::Unknown,
        }
    }
}
