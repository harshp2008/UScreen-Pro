//! System tray icon.
//!
//! The daemon starts with the desktop, which means it otherwise runs with
//! nothing to show for itself: no way to see whether a tablet is attached, and
//! no way to change what it is doing without opening the GUI or a terminal.
//!
//! This is a StatusNotifierItem over D-Bus. On a Wayland session that is the
//! only tray protocol there is — XEmbed is X11-only — so there is no fallback
//! to write, and a desktop without a StatusNotifierWatcher simply gets no icon.
//! That is not an error worth failing the daemon over.

use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
use ksni::{Handle, Status, ToolTip, Tray, TrayMethods};
use tokio::sync::watch;
use tracing::{info, warn};

/// The emerald UScreen Pro icons, embedded as pixmaps. The tray deliberately names no theme icon:
/// an old `uscreen` icon left in the hicolor theme by a previous install would otherwise win over the
/// pixmap and show the retired logo. The pen variant carries the mode at a glance without a tooltip.
const PIXMAP_SCREEN: &[u8] = include_bytes!("../../../packaging/icons/uscreen-64.rgba");
const PIXMAP_TABLET: &[u8] = include_bytes!("../../../packaging/icons/uscreen-pen-64.rgba");
const PIXMAP_SIDE: i32 = 64;

/// RGBA as ImageMagick wrote it → ARGB32 in network byte order, which is
/// what StatusNotifierItem expects.
fn pixmap(rgba: &[u8]) -> ksni::Icon {
    let mut data = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    ksni::Icon {
        width: PIXMAP_SIDE,
        height: PIXMAP_SIDE,
        data,
    }
}

/// The Tauri settings app. The retired `uscreen-gui` must never be launched from here.
const GUI_BIN: &str = "uscreen-pro-gui";

struct UScreenTray {
    pen_only: bool,
    tablet_present: bool,
    /// Newer release, when the daily check found one.
    update: Option<String>,
    /// Asks the daemon to make Stylus mode (true) or Display mode (false) the global default.
    mode_request_tx: tokio::sync::mpsc::UnboundedSender<bool>,
    shutdown_tx: watch::Sender<bool>,
}

impl UScreenTray {
    /// The icon for the current mode (tablet or screen).
    fn current_pixmap(&self) -> ksni::Icon {
        pixmap(if self.pen_only {
            PIXMAP_TABLET
        } else {
            PIXMAP_SCREEN
        })
    }

    /// One line saying what the tray's tablet is doing.
    fn state_line(&self) -> String {
        if !self.tablet_present {
            "No tablet connected".to_string()
        } else if self.pen_only {
            "Graphics tablet — the pen drives this screen".to_string()
        } else {
            "Second screen".to_string()
        }
    }
}

impl Tray for UScreenTray {
    fn id(&self) -> String {
        "uscreen-pro".into()
    }

    fn title(&self) -> String {
        "UScreen Pro".into()
    }

    /// Empty on purpose, so the desktop uses [`Self::icon_pixmap`] (see the pixmap constants).
    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.current_pixmap()]
    }

    /// Always visible. Passive lets the desktop hide the icon, and an icon you
    /// have to go looking for defeats the point of adding one.
    fn status(&self) -> Status {
        Status::Active
    }

    fn tool_tip(&self) -> ToolTip {
        let mut description = self.state_line();
        if let Some(v) = &self.update {
            description.push_str(&format!("\nUpdate available: {}", v));
        }
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: vec![self.current_pixmap()],
            title: "UScreen Pro".into(),
            description,
        }
    }

    /// Left click opens the settings window, which is what a tray icon for a
    /// background service is normally expected to do.
    fn activate(&mut self, _x: i32, _y: i32) {
        open_settings();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = vec![StandardItem {
            label: self.state_line(),
            enabled: false,
            ..Default::default()
        }
        .into()];
        if let Some(v) = &self.update {
            items.push(
                StandardItem {
                    label: format!("Update available: {} — open release page", v),
                    icon_name: "system-software-update".into(),
                    activate: Box::new(|_: &mut Self| open_release_page()),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.extend([
            MenuItem::Separator,
            CheckmarkItem {
                label: "Graphics tablet".into(),
                checked: self.pen_only,
                // Settable with no tablet attached. The mode is a persistent
                // choice, not an action on a live device: it decides what the
                // tablet will be the moment it is plugged in, so greying it
                // out would take away the one chance to set it in advance.
                activate: Box::new(|t: &mut Self| {
                    let want = !t.pen_only;
                    // Only ask: the daemon writes the global default, the config
                    // watcher applies it, and the update task below sets the
                    // checkmark from slot 0's real mode, so the menu can only
                    // ever show a mode the daemon actually entered.
                    let _ = t.mode_request_tx.send(want);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Show Window".into(),
                icon_name: "configure".into(),
                activate: Box::new(|_: &mut Self| open_settings()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Stop Daemon (Quit)".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| {
                    info!("Quit requested from the tray");
                    let _ = t.shutdown_tx.send(true);
                }),
                ..Default::default()
            }
            .into(),
        ]);
        items
    }
}

/// Open the releases page in the default browser.
fn open_release_page() {
    match std::process::Command::new("xdg-open")
        .arg(crate::daemon::update::RELEASES_PAGE)
        .spawn()
    {
        Ok(_) => info!("Opened the release page"),
        Err(e) => warn!(
            "Could not open {}: {}",
            crate::daemon::update::RELEASES_PAGE,
            e
        ),
    }
}

/// Best effort: a missing GUI binary is worth a log line, not a crash in the
/// middle of a menu callback.
fn open_settings() {
    // A second window of the same app is never what a click on the tray means.
    let running = std::process::Command::new("pgrep")
        .args(["-x", GUI_BIN])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if running {
        // Ask the window manager to raise the existing window; harmless if wmctrl is absent.
        let _ = std::process::Command::new("wmctrl")
            .args(["-x", "-a", GUI_BIN])
            .status();
        info!("{} is already running; raised its window", GUI_BIN);
        return;
    }
    // Next to this binary first. Under systemd --user the PATH usually lacks
    // ~/.local/bin, where the installer puts both binaries, so a bare name would
    // fail for exactly the installs that start the daemon at login.
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(GUI_BIN)))
        .filter(|p| p.exists());
    let program = sibling.unwrap_or_else(|| std::path::PathBuf::from(GUI_BIN));
    // The window must outlive "Stop Daemon". A plain child of this process sits
    // in uscreen.service's cgroup, and `systemctl stop` (KillMode=control-group)
    // kills it along with the daemon, so stopping the daemon from inside the GUI
    // closed the GUI (RCA 58). Start it as its own transient unit instead.
    let unit = format!(
        "uscreen-pro-gui-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let mut sr = std::process::Command::new("systemd-run");
    sr.args(["--user", "--collect", "--quiet", "--unit", &unit]);
    for var in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        if std::env::var_os(var).is_some() {
            sr.arg(format!("--setenv={var}"));
        }
    }
    sr.arg(&program);
    match sr.status() {
        Ok(s) if s.success() => {
            info!("Opened settings from the tray (detached unit {})", unit);
            return;
        }
        other => warn!(
            "systemd-run could not detach {} ({:?}); starting it directly",
            GUI_BIN, other
        ),
    }
    match std::process::Command::new(&program).spawn() {
        Ok(_) => info!("Opened settings from the tray"),
        Err(e) => warn!("Could not launch {}: {}", GUI_BIN, e),
    }
}

/// Publish the tray icon and keep it in step with the daemon.
///
/// Returns without an icon if no StatusNotifierWatcher answers — a desktop
/// with no tray is a perfectly ordinary thing to run on.
pub async fn run(
    mode_tx: watch::Sender<bool>,
    mode_request_tx: tokio::sync::mpsc::UnboundedSender<bool>,
    mut tablet_rx: watch::Receiver<bool>,
    shutdown_tx: watch::Sender<bool>,
    mut update_rx: watch::Receiver<crate::daemon::update::Available>,
) {
    let mut mode_rx = mode_tx.subscribe();
    let mut shutdown_rx = shutdown_tx.subscribe();

    // The daemon starts with the session, usually before gnome-shell's
    // StatusNotifierWatcher (the AppIndicator extension) is on the bus. Giving
    // up on the first ServiceUnknown left the tray empty for the whole session,
    // so keep trying until a watcher answers.
    let mut delay = std::time::Duration::from_millis(500);
    let mut warned = false;
    let handle: Handle<UScreenTray> = loop {
        let tray = UScreenTray {
            pen_only: *mode_rx.borrow_and_update(),
            tablet_present: *tablet_rx.borrow_and_update(),
            update: update_rx.borrow_and_update().clone(),
            mode_request_tx: mode_request_tx.clone(),
            shutdown_tx: shutdown_tx.clone(),
        };
        match tray.spawn().await {
            Ok(h) => {
                info!("Tray icon registered");
                break h;
            }
            Err(e) => {
                if !warned {
                    warned = true;
                    info!(
                        "No StatusNotifierWatcher yet ({}); retrying until one appears",
                        e
                    );
                }
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = shutdown_rx.changed() => return,
                }
                delay = (delay * 2).min(std::time::Duration::from_secs(5));
            }
        }
    };

    // Follow both signals for the lifetime of the daemon. The menu is rebuilt
    // from this state on every open, so an update here is all it takes for the
    // icon, the tooltip and the checkmark to agree with reality.
    loop {
        tokio::select! {
            r = mode_rx.changed() => {
                if r.is_err() { break; }
                let pen_only = *mode_rx.borrow();
                handle.update(move |t: &mut UScreenTray| t.pen_only = pen_only).await;
            }
            r = tablet_rx.changed() => {
                if r.is_err() { break; }
                let present = *tablet_rx.borrow();
                handle.update(move |t: &mut UScreenTray| t.tablet_present = present).await;
            }
            r = update_rx.changed() => {
                if r.is_err() { break; }
                let v = update_rx.borrow().clone();
                handle.update(move |t: &mut UScreenTray| t.update = v).await;
            }
        }
    }
}
