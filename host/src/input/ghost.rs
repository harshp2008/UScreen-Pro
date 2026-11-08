//! Ghost tracking: one pen, two virtual devices.
//!
//! On Wayland (Mutter, verified in 46.2 `meta-backend.c`) a tablet-class event hides the regular
//! pointer unless a pointer-class event arrived in the last 300 ms, and clients that do not bind the
//! tablet protocol draw no cursor for the tool. A pen on `UScreen Pen` alone therefore has an
//! invisible cursor and cannot select text. So every pen sample is also written to `UScreen Pointer`
//! (hover and move included), which keeps one ordinary, visible cursor under the pen. Only the
//! *tip* is routed: pressure-aware apps (the art list, or a forced override) get tip down/up on the
//! pen; everything else gets `BTN_LEFT` on the pointer.
//!
//! The same code serves the Android pen and a physical drawing tablet read through evdev (see
//! `physical`). On X11 the tablet already drives the single core pointer, so nothing here applies.
//!
//! Optional idle hide: after the pen has been *out of range* for `stylus_idle_hide_timeout` seconds
//! one tablet-class frame (proximity in/out) is sent with no pointer motion, and Mutter hides the
//! pointer until the next pointer-class event.

use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// One pen sample in the injector's units: position 0..=COORD_MAX over the pen's output, pressure
/// 0..=PEN_PRESSURE_MAX, tilt in degrees.
///
/// `action` is the pen protocol's: 0 down, 1 up, 2 move (touching), 3 hover, 4 left range,
/// 5/6 first side button down/up, 7/8 second side button down/up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PenSample {
    pub x: i32,
    pub y: i32,
    pub pressure: i32,
    pub tilt_x: i32,
    pub tilt_y: i32,
    pub eraser: bool,
    pub action: u8,
}

/// Where the tip of the current stroke goes. Fixed at pen-down so a focus change mid-stroke cannot
/// leave a button or a tablet contact stuck on the wrong device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Tip {
    #[default]
    None,
    Pen,
    Pointer,
}

/// Whether the session is X11, where ghost tracking is unnecessary (one core pointer).
pub(super) fn session_is_x11() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("x11"))
}

// ---------------------------------------------------------------------------------------------
// Focus: which app the pen is drawing into, and the user's per-app overrides.
// ---------------------------------------------------------------------------------------------

/// Identity of the focused window, any of which may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub(super) struct AppIdentity {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub wm_class: String,
    #[serde(default)]
    pub gtk_id: String,
}

impl AppIdentity {
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
        self.names()
            .any(|n| art_apps.iter().any(|a| !a.is_empty() && n.contains(a.as_str())))
    }
}

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
pub(super) fn route_to_pen(art_apps: &[String]) -> bool {
    let f = focus().lock().unwrap_or_else(|e| e.into_inner());
    decide(f.app.as_ref(), &f.overrides, f.unknown_override, art_apps)
}

fn decide(
    app: Option<&AppIdentity>,
    overrides: &HashMap<String, bool>,
    unknown_override: Option<bool>,
    art_apps: &[String],
) -> bool {
    match app.and_then(|a| a.key().map(|k| (a, k))) {
        Some((a, key)) => overrides.get(&key).copied().unwrap_or_else(|| a.is_art(art_apps)),
        None => unknown_override.unwrap_or(false),
    }
}

/// Ctrl+Alt+P: flip the route for the focused app (or for "unknown" when no focus source exists).
/// Returns the new route (`true` = tablet).
pub(super) fn toggle_route(art_apps: &[String]) -> bool {
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

fn set_focus(app: Option<AppIdentity>) {
    focus().lock().unwrap_or_else(|e| e.into_inner()).app = app;
}

/// `~/.local/share/uscreen/<name>`, next to `app_lock.json`.
fn data_file(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".local/share/uscreen").join(name))
}

/// The file the Shell extension writes on every focus change.
pub fn focus_file() -> Option<std::path::PathBuf> {
    data_file("focus.json")
}

/// The file the extension (hotkey) and `uscreen toggle-tablet` write to flip the route.
pub fn toggle_file() -> Option<std::path::PathBuf> {
    data_file("toggle_request")
}

/// Start the focus watcher once per process: it follows the focused app (extension file on
/// Wayland, `_NET_ACTIVE_WINDOW` on X11) and the toggle request file.
pub(super) fn start_focus_watcher() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("uscreen-focus".into())
        .spawn(|| {
            let mut last_focus_mtime = None;
            let mut last_toggle = std::fs::read_to_string(toggle_file().unwrap_or_default()).ok();
            let mut x11 = if session_is_x11() { X11Focus::connect() } else { None };
            loop {
                std::thread::sleep(Duration::from_millis(150));
                if let Some(x) = x11.as_mut() {
                    set_focus(x.active());
                } else if let Some(path) = focus_file() {
                    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                    if mtime != last_focus_mtime {
                        last_focus_mtime = mtime;
                        let app = std::fs::read_to_string(&path)
                            .ok()
                            .and_then(|t| serde_json::from_str::<AppIdentity>(&t).ok());
                        set_focus(app);
                    }
                }
                if let Some(path) = toggle_file() {
                    let now = std::fs::read_to_string(&path).ok();
                    if now != last_toggle {
                        last_toggle = now;
                        if last_toggle.is_some() {
                            let art = crate::config::FileConfig::load().stylus_art_apps;
                            let tablet = toggle_route(&art);
                            info!(
                                "Pen route for the focused app is now {}",
                                if tablet { "tablet (pressure)" } else { "mouse" }
                            );
                        }
                    }
                }
            }
        });
    if let Err(e) = spawned {
        warn!("Could not start the focus watcher: {}", e);
    }
}

/// `_NET_ACTIVE_WINDOW` and `WM_CLASS` over x11rb, for X11 sessions.
struct X11Focus {
    conn: x11rb::rust_connection::RustConnection,
    root: u32,
    active_atom: u32,
}

impl X11Focus {
    fn connect() -> Option<Self> {
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

// ---------------------------------------------------------------------------------------------
// Injection.
// ---------------------------------------------------------------------------------------------

/// After the cursor-hiding blip, how long Mutter's pointer-motion grace window (300 ms) has
/// certainly passed. The idle timeout is at least one second, so this only guards the tests.
const MUTTER_POINTER_GRACE: Duration = Duration::from_millis(300);

impl InjectDevices {
    /// Put the absolute pointer under the pen (no button): the visible cursor.
    ///
    /// While the tablet tool is in range its events hide the pointer unless a pointer-class event
    /// came in the last 300 ms, but the kernel drops an absolute event whose value did not change.
    /// A pen held still (a real tablet still reports tilt and pressure noise) would therefore let
    /// the cursor vanish; so when the tool is up and the position repeats, it is written one unit
    /// (1/65535 of the desktop, far below a pixel) to the side and back.
    fn ghost_place_pointer(&mut self, x: i32, y: i32) {
        let (px, py) = self.desktop_xy(x, y);
        let mut wx = px;
        if self.pen_proximity && self.ghost_last_written == (px, py) {
            wx = if px < COORD_MAX { px + 1 } else { px - 1 };
        }
        self.ghost_last_written = (wx, py);
        tracing::debug!("Injected absolute mouse to {},{}", wx, py);
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_ABS, ABS_X, wx);
            let _ = dev.emit(EV_ABS, ABS_Y, py);
            let _ = dev.syn();
        }
    }

    fn ghost_pointer_button(&mut self, code: u16, bit: u8, down: bool) {
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_KEY, code, down as i32);
            let _ = dev.syn();
        }
        if down {
            self.pen_mouse_buttons |= bit;
        } else {
            self.pen_mouse_buttons &= !bit;
        }
    }

    /// Feed one sample to both devices.
    ///
    /// `to_pen` says whether the focused app wants the pressure pen (see [`route_to_pen`]); with no
    /// focus information it is `false`, so the pen is an ordinary mouse out of the box.
    ///
    /// The pointer follows every sample (hover, move, down, up), in the same call and with the same
    /// coordinates as the tablet device. The tablet tool, though, is only *in range* while the route
    /// is the pen: clients (Chrome, Qt/Krita) that see a tool in range treat the session as a pen's
    /// and mishandle a plain mouse button pressed beside it (measured: quick taps were dropped when
    /// the tool re-entered range with each button press). In the mouse route the tool stays away.
    pub(super) fn ghost_pen(&mut self, s: &PenSample, to_pen: bool, primary: &str, secondary: &str) {
        // Any sample ends an out-of-range wait, and brings a hidden cursor back (the pointer
        // motion below is what shows it).
        if s.action != 4 {
            self.ghost_out_since = None;
            self.ghost_hidden = false;
        }
        let pressure_contact = s.pressure.clamp(1, PEN_PRESSURE_MAX);
        match s.action {
            0 => {
                self.ghost_place_pointer(s.x, s.y);
                // A stroke that was still open (a lost up) is closed first.
                self.ghost_end_stroke(s);
                self.tip = if to_pen {
                    Tip::Pen
                } else if s.eraser {
                    Tip::None
                } else {
                    Tip::Pointer
                };
                match self.tip {
                    Tip::Pen => self.ghost_tablet(s, 0),
                    Tip::Pointer => {
                        self.ghost_tool_away(s);
                        self.ghost_pointer_button(BTN_LEFT, 1, true);
                    }
                    Tip::None => self.ghost_tool(s, to_pen),
                }
                self.last_pen_pos = (s.x, s.y);
            }
            2 => {
                self.ghost_place_pointer(s.x, s.y);
                match self.tip {
                    Tip::Pen => {
                        let mut m = *s;
                        m.pressure = pressure_contact;
                        self.ghost_tablet(&m, 2);
                    }
                    Tip::Pointer => self.ghost_tool_away(s),
                    Tip::None => self.ghost_tool(s, to_pen),
                }
                self.last_pen_pos = (s.x, s.y);
            }
            3 => {
                self.ghost_place_pointer(s.x, s.y);
                self.ghost_tool(s, to_pen);
                self.last_pen_pos = (s.x, s.y);
            }
            1 => {
                self.ghost_place_pointer(s.x, s.y);
                self.ghost_end_stroke(s);
                // Whatever the lift did not manage to let go of, let go of now.
                self.ghost_panic_release(false);
                self.last_pen_pos = (s.x, s.y);
            }
            4 => {
                self.ghost_end_stroke(s);
                self.ghost_tool_away(s);
                self.ghost_panic_release(false);
                self.ghost_out_since = Some(Instant::now());
            }
            5 | 6 if to_pen => self.ghost_tablet(s, s.action),
            5..=8 => {
                let name = if s.action <= 6 { primary } else { secondary };
                let down = s.action == 5 || s.action == 7;
                let (code, bit) = match name {
                    "right" => (BTN_RIGHT, 2u8),
                    "middle" => (BTN_MIDDLE, 4u8),
                    _ => return,
                };
                self.ghost_pointer_button(code, bit, down);
            }
            _ => {}
        }
    }

    /// The tablet tool follows the pen only on the pen route; on the mouse route it is kept out of range.
    fn ghost_tool(&mut self, s: &PenSample, to_pen: bool) {
        if to_pen {
            self.ghost_tablet(s, 3);
        } else {
            self.ghost_tool_away(s);
        }
    }

    /// Take the tablet tool out of range if it is in range (a route flip, or the start of a click).
    fn ghost_tool_away(&mut self, s: &PenSample) {
        if self.pen_proximity {
            self.ghost_tablet(s, 4);
        }
    }

    /// Write a pen frame for `action` to the tablet device, keeping the proximity bookkeeping.
    fn ghost_tablet(&mut self, s: &PenSample, action: u8) {
        let Some(ref mut dev) = self.pen else { return };
        let r = dev.inject_pen(s.x, s.y, s.pressure, s.tilt_x, s.tilt_y, action, s.eraser);
        if let Err(e) = r {
            warn!("Failed to inject pen: {}", e);
            return;
        }
        match action {
            0 | 3 => self.pen_proximity = true,
            1 | 4 => self.pen_proximity = false,
            5 => self.pen_button = true,
            6 => self.pen_button = false,
            _ => {}
        }
    }

    /// Finish whatever the current stroke holds: lift the pen contact or release `BTN_LEFT`.
    fn ghost_end_stroke(&mut self, s: &PenSample) {
        match std::mem::take(&mut self.tip) {
            Tip::Pen => {
                // Lift on the tablet. It leaves proximity, as a lifted tool does; the next hover
                // sample brings it back.
                self.ghost_tablet(s, 1);
            }
            Tip::Pointer => {
                self.ghost_pointer_button(BTN_LEFT, 1, false);
            }
            Tip::None => {}
        }
    }

    /// Panic release: let go of everything the pen could be holding on **both** virtual devices, so
    /// a compositor that missed an "up" cannot stay in an implicit grab (the cursor stuck to the
    /// pen after switching to a mouse). The kernel drops a release for a key that is not down, so
    /// this costs nothing when nothing is stuck.
    ///
    /// `BTN_LEFT` and the tablet tip/tool always go. The right and middle buttons and the pen's
    /// barrel button follow their own press/release events (Android deliberately sends a range
    /// exit with the barrel still held just before the tip lands), so they are only forced up with
    /// `everything` (cancel, disconnect, mode change).
    pub(super) fn ghost_panic_release(&mut self, everything: bool) {
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_KEY, BTN_LEFT, 0);
            if everything {
                let _ = dev.emit(EV_KEY, BTN_RIGHT, 0);
                let _ = dev.emit(EV_KEY, BTN_MIDDLE, 0);
            }
            let _ = dev.syn();
        }
        self.pen_mouse_buttons &= if everything { 0 } else { !1u8 };
        if let Some(ref mut dev) = self.pen {
            let _ = dev.emit(EV_KEY, BTN_TOUCH, 0);
            if everything {
                let _ = dev.emit(EV_KEY, BTN_STYLUS, 0);
            }
            let _ = dev.emit(EV_KEY, BTN_TOOL_PEN, 0);
            let _ = dev.emit(EV_KEY, BTN_TOOL_RUBBER, 0);
            let _ = dev.emit(EV_ABS, ABS_PRESSURE, 0);
            let _ = dev.syn();
        }
        self.pen_proximity = false;
        if everything {
            self.pen_button = false;
        }
        self.tip = Tip::None;
    }

    /// Called a few times a second: once the pen has been out of range for `timeout_secs`
    /// (0 = never), hide the cursor by sending a tablet-class frame with no pointer motion.
    /// Returns whether it hid the cursor now.
    pub(super) fn ghost_idle_tick(&mut self, timeout_secs: u32) -> bool {
        let Some(since) = self.ghost_out_since else { return false };
        if timeout_secs == 0 || self.ghost_hidden || self.tip != Tip::None {
            return false;
        }
        let wait = Duration::from_secs(timeout_secs as u64).max(MUTTER_POINTER_GRACE * 2);
        if since.elapsed() < wait {
            return false;
        }
        let (x, y) = self.last_pen_pos;
        if let Some(ref mut dev) = self.pen {
            // Proximity in at the last position, then out: two tablet-class frames, no pointer-class
            // event. More than 300 ms after the last pointer motion, Mutter hides the pointer.
            let _ = dev.emit(EV_KEY, BTN_TOOL_PEN, 1);
            let _ = dev.emit(EV_ABS, ABS_X, x);
            let _ = dev.emit(EV_ABS, ABS_Y, y);
            let _ = dev.syn();
            let _ = dev.emit(EV_KEY, BTN_TOOL_PEN, 0);
            let _ = dev.syn();
        }
        self.ghost_hidden = true;
        true
    }
}

/// A physical tablet's sample: always ghost tracking (the tablet exists to be a pen with a visible
/// cursor), whatever the Android pen's `stylus_style` is, unless X11 or no pointer device.
pub(super) fn apply_physical(
    guard: &mut InjectDevices,
    opts: &crate::options::DeviceOptions,
    s: &PenSample,
) {
    if guard.has_pointer() && !session_is_x11() {
        let to_pen = route_to_pen(&opts.stylus_art_apps);
        guard.ghost_pen(s, to_pen, &opts.stylus_button_primary, &opts.stylus_button_secondary);
    } else {
        super::events::apply_pen(guard, opts, s);
    }
}

/// Drive the idle hide for the daemon's lifetime: a few ticks a second, reading the timeout live.
pub(super) fn spawn_idle_task(uinput: Arc<std::sync::Mutex<InjectDevices>>, instance: u32) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tick.tick().await;
            let timeout = crate::options::current_options(instance).stylus_idle_hide_timeout;
            if timeout == 0 {
                continue;
            }
            if let Ok(mut guard) = uinput.lock() {
                if guard.ghost_idle_tick(timeout) {
                    info!("Pen out of range for {}s — cursor hidden", timeout);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, class: &str) -> AppIdentity {
        AppIdentity { app_id: id.into(), wm_class: class.into(), gtk_id: String::new() }
    }

    fn art() -> Vec<String> {
        crate::config::default_art_apps()
    }

    #[test]
    fn art_apps_match_by_any_name_case_insensitively() {
        assert!(app("org.kde.krita.desktop", "krita").is_art(&art()));
        assert!(app("", "Gimp-2.10").is_art(&art()));
        assert!(!app("google-chrome.desktop", "Google-chrome").is_art(&art()));
        assert!(!AppIdentity::default().is_art(&art()));
    }

    #[test]
    fn route_follows_the_app_then_the_override() {
        let krita = app("org.kde.krita.desktop", "krita");
        let chrome = app("google-chrome.desktop", "Google-chrome");
        let mut ov = HashMap::new();
        assert!(decide(Some(&krita), &ov, None, &art()));
        assert!(!decide(Some(&chrome), &ov, None, &art()));
        ov.insert(chrome.key().unwrap(), true);
        assert!(decide(Some(&chrome), &ov, None, &art()), "forced tablet for Chrome");
        ov.insert(krita.key().unwrap(), false);
        assert!(!decide(Some(&krita), &ov, None, &art()), "forced mouse for Krita");
        assert!(!decide(None, &ov, None, &art()), "unknown focus defaults to the mouse");
        assert!(decide(None, &ov, Some(true), &art()));
    }

    #[test]
    fn toggling_remembers_the_choice_per_app() {
        set_focus(Some(app("zz-test-app.desktop", "zz-test-app")));
        assert!(!route_to_pen(&art()));
        assert!(toggle_route(&art()));
        assert!(route_to_pen(&art()));
        set_focus(Some(app("other", "other")));
        assert!(!route_to_pen(&art()), "another app is untouched");
        set_focus(Some(app("zz-test-app.desktop", "zz-test-app")));
        assert!(route_to_pen(&art()), "remembered");
        assert!(!toggle_route(&art()));
    }

    fn rig() -> (InjectDevices, std::path::PathBuf, std::path::PathBuf) {
        // Tests run in parallel threads of one process: every rig needs its own files.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mk = |n: &str| {
            let path = std::env::temp_dir().join(format!("uscreen-ghost-{}-{}-{}", n, std::process::id(), id));
            let file = OpenOptions::new().create(true).truncate(true).write(true).open(&path).unwrap();
            (UInputDevice { file }, path)
        };
        let (pen, pp) = mk("pen");
        let (pointer, qp) = mk("pointer");
        let mut d = InjectDevices::headless();
        d.pen = Some(pen);
        d.pointer = Some(pointer);
        (d, pp, qp)
    }

    fn frames(path: &std::path::Path) -> Vec<Vec<(u16, u16, i32)>> {
        let bytes = std::fs::read(path).unwrap();
        let (mut all, mut frame) = (Vec::new(), Vec::new());
        for ev in bytes.as_chunks::<{ std::mem::size_of::<LinuxInputEvent>() }>().0 {
            let kind = u16::from_ne_bytes(ev[16..18].try_into().unwrap());
            let code = u16::from_ne_bytes(ev[18..20].try_into().unwrap());
            let value = i32::from_ne_bytes(ev[20..24].try_into().unwrap());
            if kind == EV_SYN {
                all.push(std::mem::take(&mut frame));
            } else {
                frame.push((kind, code, value));
            }
        }
        all
    }

    fn sample(action: u8, x: i32) -> PenSample {
        PenSample { x, y: 500, pressure: 2000, tilt_x: 3, tilt_y: -3, eraser: false, action }
    }

    fn has(f: &[Vec<(u16, u16, i32)>], e: (u16, u16, i32)) -> bool {
        f.iter().any(|fr| fr.contains(&e))
    }

    fn done(p: &[&std::path::Path]) {
        for x in p {
            let _ = std::fs::remove_file(x);
        }
    }

    #[test]
    fn hover_moves_both_devices_on_the_pen_route_so_the_cursor_stays_visible() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(3, 100), true, "right", "middle");
        d.ghost_pen(&sample(3, 200), true, "right", "middle");
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert_eq!(pen.len(), 2);
        assert_eq!(ptr.len(), 2, "one pointer motion frame per hover sample");
        assert!(has(&pen, (EV_ABS, ABS_X, 200)) && has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)));
        assert!(has(&ptr, (EV_ABS, ABS_X, 200)));
        assert!(!has(&ptr, (EV_KEY, BTN_LEFT, 1)), "hover never clicks");
    }

    #[test]
    fn the_mouse_route_never_puts_the_tablet_tool_in_range() {
        // The Android sequence of a tap: hover, range exit (just before the tip lands), down, a
        // move, up, hover again - then a second tap.
        let (mut d, pp, qp) = rig();
        for _ in 0..2 {
            d.ghost_pen(&sample(3, 100), false, "right", "middle");
            d.ghost_pen(&sample(4, 0), false, "right", "middle");
            d.ghost_pen(&sample(0, 100), false, "right", "middle");
            d.ghost_pen(&sample(2, 110), false, "right", "middle");
            d.ghost_pen(&sample(1, 110), false, "right", "middle");
        }
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert!(!has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)), "no proximity-in, ever: clients would treat it as a pen session");
        assert!(!has(&pen, (EV_KEY, BTN_TOUCH, 1)));
        assert!(!pen.iter().flatten().any(|e| e.0 == EV_ABS && e.1 == ABS_X), "no tablet motion either");
        let left: Vec<i32> = ptr.iter().flat_map(|f| f.iter().filter(|e| e.1 == BTN_LEFT && e.2 == 1).map(|e| e.2)).collect();
        assert_eq!(left.len(), 2, "both taps click");
        assert!(has(&ptr, (EV_ABS, ABS_X, 110)));
        assert_eq!(d.pen_mouse_buttons, 0);
    }

    #[test]
    fn a_route_flip_while_hovering_moves_the_tool_in_and_out_of_range() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(3, 100), true, "right", "middle");
        assert!(d.pen_proximity);
        d.ghost_pen(&sample(3, 110), false, "right", "middle");
        assert!(!d.pen_proximity, "left the pen route: the tool leaves range");
        let pen = frames(&pp);
        done(&[&pp, &qp]);
        assert!(has(&pen, (EV_KEY, BTN_TOOL_PEN, 0)));
    }

    #[test]
    fn a_still_pen_keeps_the_pointer_alive_with_a_one_unit_nudge() {
        let (mut d, pp, qp) = rig();
        for _ in 0..3 {
            d.ghost_pen(&sample(3, 500), true, "right", "middle");
        }
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        let xs: Vec<i32> = ptr.iter().flat_map(|f| f.iter().filter(|e| e.1 == ABS_X).map(|e| e.2)).collect();
        assert_eq!(xs, vec![500, 501, 500], "a repeat is written beside the position, never twice the same");
    }

    #[test]
    fn panic_release_lets_go_of_everything_on_both_devices() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(0, 100), false, "right", "middle"); // left held
        d.ghost_pen(&sample(5, 0), false, "right", "middle"); // barrel -> right held
        assert_eq!(d.pen_mouse_buttons, 1 | 2);
        d.ghost_panic_release(false);
        assert_eq!(d.pen_mouse_buttons, 2, "the barrel button keeps its own lifecycle");
        d.ghost_panic_release(true);
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert_eq!(d.pen_mouse_buttons, 0);
        assert_eq!(d.tip, Tip::None);
        for e in [(EV_KEY, BTN_LEFT, 0), (EV_KEY, BTN_RIGHT, 0), (EV_KEY, BTN_MIDDLE, 0)] {
            assert!(has(&ptr, e), "pointer: {e:?}");
        }
        for e in [(EV_KEY, BTN_TOUCH, 0), (EV_KEY, BTN_STYLUS, 0), (EV_KEY, BTN_TOOL_PEN, 0), (EV_KEY, BTN_TOOL_RUBBER, 0)] {
            assert!(has(&pen, e), "pen: {e:?}");
        }
    }

    #[test]
    fn a_lift_with_no_matching_down_still_releases_the_left_button() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(1, 100), false, "right", "middle");
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert!(has(&ptr, (EV_KEY, BTN_LEFT, 0)));
    }

    #[test]
    fn non_art_app_tip_is_a_left_click_on_the_pointer_only() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(3, 100), false, "right", "middle");
        d.ghost_pen(&sample(0, 100), false, "right", "middle");
        for i in 1..=5 {
            d.ghost_pen(&sample(2, 100 + i * 10), false, "right", "middle");
        }
        d.ghost_pen(&sample(1, 150), false, "right", "middle");
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        let left: Vec<i32> = ptr
            .iter()
            .flat_map(|f| f.iter().filter(|e| e.1 == BTN_LEFT).map(|e| e.2))
            .collect();
        assert_eq!(left.iter().filter(|v| **v == 1).count(), 1, "one press");
        assert_eq!(left.first(), Some(&1));
        assert_eq!(left.last(), Some(&0), "and it ends released (a redundant release is dropped by the kernel)");
        assert!(!has(&pen, (EV_KEY, BTN_TOUCH, 1)), "the tablet never touches");
        assert!(
            pen.iter().all(|f| !f.contains(&(EV_ABS, ABS_PRESSURE, 2000))),
            "no pressure reaches the tablet in a non-art app"
        );
        assert!(!has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)), "and it stays out of range");
        assert!(has(&ptr, (EV_ABS, ABS_X, 150)), "the pointer follows to the lift point");
        assert_eq!(d.pen_mouse_buttons, 0);
    }

    #[test]
    fn art_app_tip_goes_to_the_pen_with_pressure_and_never_clicks() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(3, 100), true, "right", "middle");
        d.ghost_pen(&sample(0, 100), true, "right", "middle");
        d.ghost_pen(&sample(2, 120), true, "right", "middle");
        d.ghost_pen(&sample(1, 130), true, "right", "middle");
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert!(has(&pen, (EV_KEY, BTN_TOUCH, 1)) && has(&pen, (EV_KEY, BTN_TOUCH, 0)));
        assert!(has(&pen, (EV_ABS, ABS_PRESSURE, 2000)));
        assert!(
            !ptr.iter().flatten().any(|e| e.0 == EV_KEY && e.2 == 1),
            "the pointer presses no button"
        );
        assert!(has(&ptr, (EV_ABS, ABS_X, 130)), "but follows the pen to the lift point");
    }

    #[test]
    fn a_stroke_stays_on_the_device_it_started_on_when_the_route_flips() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(0, 100), false, "right", "middle"); // started as a mouse click
        d.ghost_pen(&sample(2, 120), true, "right", "middle"); // the route flips mid-stroke
        d.ghost_pen(&sample(1, 130), true, "right", "middle");
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        let left: Vec<i32> = ptr
            .iter()
            .flat_map(|f| f.iter().filter(|e| e.1 == BTN_LEFT).map(|e| e.2))
            .collect();
        assert_eq!(left.iter().filter(|v| **v == 1).count(), 1);
        assert_eq!(left.last(), Some(&0), "released on the pointer");
        assert!(!has(&pen, (EV_KEY, BTN_TOUCH, 1)), "never touched the tablet");
    }

    #[test]
    fn side_button_goes_to_the_pen_in_art_apps_and_to_the_pointer_elsewhere() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(5, 0), true, "right", "middle");
        d.ghost_pen(&sample(6, 0), true, "right", "middle");
        d.ghost_pen(&sample(5, 0), false, "right", "middle");
        d.ghost_pen(&sample(6, 0), false, "right", "middle");
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert!(has(&pen, (EV_KEY, BTN_STYLUS, 1)) && has(&pen, (EV_KEY, BTN_STYLUS, 0)));
        assert!(has(&ptr, (EV_KEY, BTN_RIGHT, 1)) && has(&ptr, (EV_KEY, BTN_RIGHT, 0)));
        assert_eq!(d.pen_mouse_buttons, 0);
    }

    #[test]
    fn leaving_range_stops_pointer_updates_and_starts_the_idle_clock() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(3, 100), false, "right", "middle");
        d.ghost_pen(&sample(4, 100), false, "right", "middle");
        let ptr = frames(&qp);
        let pen = frames(&pp);
        done(&[&pp, &qp]);
        assert_eq!(ptr.iter().flatten().filter(|e| e.1 == ABS_X).count(), 1, "range exit moves nothing on the pointer");
        assert!(has(&pen, (EV_KEY, BTN_TOOL_PEN, 0)));
        assert!(d.ghost_out_since.is_some());
        assert!(!d.pen_proximity);
    }

    #[test]
    fn idle_hide_is_off_at_zero_and_fires_once_after_the_timeout_with_no_pointer_motion() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(3, 321), false, "right", "middle");
        d.ghost_pen(&sample(4, 321), false, "right", "middle");
        assert!(!d.ghost_idle_tick(0), "0 means never");
        assert!(!d.ghost_idle_tick(2), "too early");
        d.ghost_out_since = Some(Instant::now() - Duration::from_secs(3));
        let ptr_before = frames(&qp).len();
        assert!(d.ghost_idle_tick(2));
        assert!(!d.ghost_idle_tick(2), "only once");
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert_eq!(ptr.len(), ptr_before, "the hide never touches the pointer");
        let n = pen.len();
        assert_eq!(pen[n - 2], vec![(EV_KEY, BTN_TOOL_PEN, 1), (EV_ABS, ABS_X, 321), (EV_ABS, ABS_Y, 500)]);
        assert_eq!(pen[n - 1], vec![(EV_KEY, BTN_TOOL_PEN, 0)]);
        // Coming back shows the cursor again and re-arms the clock.
        let (mut d2, p2, q2) = rig();
        d2.ghost_out_since = Some(Instant::now() - Duration::from_secs(9));
        d2.ghost_hidden = true;
        d2.ghost_pen(&sample(3, 5), false, "right", "middle");
        assert!(d2.ghost_out_since.is_none() && !d2.ghost_hidden);
        assert!(frames(&q2).iter().flatten().any(|e| e.1 == ABS_X));
        done(&[&p2, &q2]);
    }

    #[test]
    fn range_exit_releases_a_held_tip() {
        let (mut d, pp, qp) = rig();
        d.ghost_pen(&sample(0, 10), false, "right", "middle");
        d.ghost_pen(&sample(4, 10), false, "right", "middle");
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert!(has(&ptr, (EV_KEY, BTN_LEFT, 0)));
        assert_eq!(d.pen_mouse_buttons, 0);
        assert_eq!(d.tip, Tip::None);
    }
}
