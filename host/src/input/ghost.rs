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

use super::recorder::record;
use super::stillness::HoverVerdict;
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

/// The file `uscreen debug-dump` writes to ask the daemon for the pen recorder's contents.
pub fn dump_request_file() -> Option<std::path::PathBuf> {
    data_file("dump_request")
}

/// Where the daemon writes the pen recorder's contents.
pub fn dump_file() -> Option<std::path::PathBuf> {
    data_file("pen_dump.txt")
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
            let mut last_dump = std::fs::read_to_string(dump_request_file().unwrap_or_default()).ok();
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
                if let (Some(req), Some(out)) = (dump_request_file(), dump_file()) {
                    let now = std::fs::read_to_string(&req).ok();
                    if now != last_dump {
                        last_dump = now;
                        if last_dump.is_some() {
                            let text = super::recorder::render();
                            match std::fs::write(&out, &text) {
                                Ok(()) => info!("Pen recorder written to {:?} ({} lines)", out, text.lines().count()),
                                Err(e) => warn!("Could not write the pen recorder to {:?}: {}", out, e),
                            }
                        }
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

/// How long an announced range exit waits for the pen to come back before the tool really leaves
/// range (Android's hover exit precedes the touch-down by a few milliseconds).
const AUTO_EXIT_GRACE: Duration = Duration::from_millis(150);

/// A DOWN while a stroke is open and a sample arrived this recently is the drag going on (a
/// repeated DOWN), not a new press. Older than this, the stroke lost its UP and is closed first.
const AUTO_STALE_STROKE: Duration = Duration::from_millis(500);

/// A MOVE this soon after a lift is a straggler that overtook the UP, not a stroke that lost its
/// DOWN: it must not press again.
const AUTO_LIFT_QUIET: Duration = Duration::from_millis(100);

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

    /// **Auto mode** (Android pen, `stylus_style = "auto"`): the pen is a mouse or a graphics tablet,
    /// per stroke, depending on the focused app (`to_pen`).
    ///
    /// * The pointer follows every sample (X/Y first, then the button), so the cursor is always
    ///   visible and a click lands where the pen is.
    /// * **Mouse route** (not an art app, or unknown): the tip is `BTN_LEFT` on the pointer, held for
    ///   the whole stroke; moves go to the pointer only. The tablet device gets nothing, the tool
    ///   stays out of range (a tool in range next to a mouse button makes clients treat the stroke
    ///   as a pen's and drop the drag).
    /// * **Art route**: the tablet gets the tool, X/Y, tilt, `BTN_TOUCH` and the real pressure,
    ///   unmodified; there is no `BTN_LEFT`.
    /// * The route is **latched at DOWN**: a focus change in mid-stroke never moves, cuts or
    ///   releases the stroke. Only an UP, a cancel (reaches here as an UP), a disconnect, or the
    ///   user changing the mode/style ends it. Everything is released through the tracked device
    ///   state ([`Self::force_release_all`]).
    /// * A DOWN while the stroke is open is the drag continuing; a MOVE with no stroke open (its
    ///   DOWN was lost) starts the stroke, so the drag is not lost.
    pub(super) fn auto_pen(&mut self, s: &PenSample, to_pen: bool, primary: &str, secondary: &str) {
        if s.action != 4 {
            self.ghost_out_since = None;
            self.ghost_hidden = false;
        }
        if matches!(s.action, 0..=3) {
            // The pen is (still) there: an announced exit is void.
            self.auto_exit_at = None;
        }
        if s.action != 3 {
            // Anything but a hover (touch, lift, range exit) ends a parked spell.
            self.hover_filter.reset();
        }
        let open = self.auto_down_at.is_some();
        if s.action != 3 {
            record(s, to_pen, match s.action {
                0 => "down",
                1 => "up",
                2 => "move",
                4 => "exit",
                _ => "button",
            });
        }
        match s.action {
            0 => {
                if open {
                    if self.auto_last_at.is_some_and(|t| t.elapsed() < AUTO_STALE_STROKE) {
                        // The same press again: the drag goes on, nothing is released or pressed.
                        self.auto_stroke_pointer(s);
                        self.auto_continue(s);
                        return;
                    }
                    warn!("Pen DOWN with a stale stroke open (its UP was lost): closing it first");
                    self.ghost_place_pointer(s.x, s.y);
                    self.auto_end_stroke(s);
                } else {
                    self.ghost_place_pointer(s.x, s.y);
                }
                self.auto_begin(s, to_pen);
            }
            2 => {
                if open {
                    self.auto_stroke_pointer(s);
                    self.auto_continue(s);
                } else {
                    self.ghost_place_pointer(s.x, s.y);
                    if !s.eraser && self.auto_up_at.is_none_or(|t| t.elapsed() >= AUTO_LIFT_QUIET) {
                        warn!("Pen MOVE with no stroke open (its DOWN was lost): starting the stroke");
                        self.auto_begin(s, to_pen);
                    } else {
                        self.auto_hover(s, to_pen);
                    }
                }
                self.last_pen_pos = (s.x, s.y);
            }
            3 => {
                if open {
                    // A hover inside a stroke: the cursor follows, nothing else.
                    self.auto_stroke_pointer(s);
                    self.last_pen_pos = (s.x, s.y);
                    return;
                }
                let verdict = self.hover_filter.sample(s.x, s.y, Instant::now(), self.tablet_mm, &self.still_cfg);
                match verdict {
                    // A pen resting near the glass writes nothing: the mouse keeps the cursor.
                    HoverVerdict::Skip => record(s, to_pen, "skip"),
                    HoverVerdict::Park => {
                        record(s, to_pen, "park");
                        self.ghost_tool_away(s);
                    }
                    HoverVerdict::Follow | HoverVerdict::Wake => {
                        if verdict == HoverVerdict::Wake {
                            record(s, to_pen, "wake");
                        } else {
                            record(s, to_pen, "hover");
                        }
                        self.ghost_place_pointer(s.x, s.y);
                        self.auto_hover(s, to_pen);
                        self.last_pen_pos = (s.x, s.y);
                    }
                }
            }
            1 => {
                // A lift inside the click slop releases where the tip landed.
                if !(open && self.slop_anchor.is_some()) {
                    self.ghost_place_pointer(s.x, s.y);
                }
                self.auto_end_stroke(s);
                // Whatever the lift did not manage to let go of, let go of now (only what is held:
                // the tablet device is not written to when it is idle).
                self.auto_let_go_left();
                self.auto_exit_at = None;
                self.auto_up_at = Some(Instant::now());
                self.last_pen_pos = (s.x, s.y);
            }
            4 => {
                if open {
                    // Android announces a range exit just before a touch-down, never inside a
                    // stroke; one that does arrive must not let go of a held button.
                    tracing::debug!("Pen range exit inside a stroke ignored");
                    return;
                }
                self.ghost_tool_away(s);
                self.auto_exit_at = Some(Instant::now());
                self.ghost_out_since = Some(Instant::now());
            }
            5 | 6 if self.tip == Tip::Pen || (!open && to_pen) => self.ghost_tablet(s, s.action),
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

    /// Start a stroke on the route `to_pen` says (latched until the lift). The pointer is already
    /// at the pen, so the button goes down after the position.
    fn auto_begin(&mut self, s: &PenSample, to_pen: bool) {
        self.tip = if to_pen {
            Tip::Pen
        } else if s.eraser {
            Tip::None
        } else {
            Tip::Pointer
        };
        let now = Instant::now();
        self.auto_down_at = Some(now);
        self.auto_last_at = Some(now);
        self.slop_anchor = (self.tip == Tip::Pointer).then_some((s.x, s.y));
        match self.tip {
            // Tool, X/Y, tilt, then BTN_TOUCH with the real pressure (see `inject_pen`).
            Tip::Pen => self.ghost_tablet(s, 0),
            Tip::Pointer => {
                self.ghost_tool_away(s);
                self.ghost_pointer_button(BTN_LEFT, 1, true);
            }
            Tip::None => self.ghost_tool_away(s),
        }
        self.last_pen_pos = (s.x, s.y);
    }

    /// Move the pointer for a sample of the open stroke: held at the press point while the pen stays
    /// inside the click slop, following normally from the moment it leaves it (and never held
    /// again for this stroke).
    fn auto_stroke_pointer(&mut self, s: &PenSample) {
        if let Some(anchor) = self.slop_anchor {
            let inside = super::stillness::dist_mm(anchor, (s.x, s.y), self.tablet_mm) <= self.still_cfg.click_slop_mm;
            if inside {
                return;
            }
            self.slop_anchor = None;
        }
        self.ghost_place_pointer(s.x, s.y);
    }

    /// One more sample of the open stroke, on the route it started on.
    fn auto_continue(&mut self, s: &PenSample) {
        self.auto_last_at = Some(Instant::now());
        if self.tip == Tip::Pen {
            let mut m = *s;
            m.pressure = s.pressure.clamp(1, PEN_PRESSURE_MAX);
            self.ghost_tablet(&m, 2);
        }
        self.last_pen_pos = (s.x, s.y);
    }

    /// A hovering sample: the tool follows on the art route and stays away on the mouse route.
    fn auto_hover(&mut self, s: &PenSample, to_pen: bool) {
        if to_pen {
            self.ghost_tablet(s, 3);
        } else {
            self.ghost_tool_away(s);
        }
    }

    /// The lift's safety net: the left button and the tablet contact/tool, if still held.
    fn auto_let_go_left(&mut self) {
        let left = matches!(self.dev_state(Which::Pointer), DevState::Down(m) if m & 1 != 0);
        if left {
            self.ghost_pointer_button(BTN_LEFT, 1, false);
        }
        if self.dev_state(Which::Pen) != DevState::Idle {
            self.ghost_panic_release(false);
        }
    }

    /// Close the open stroke (lift on the tablet or release the left button).
    fn auto_end_stroke(&mut self, s: &PenSample) {
        self.ghost_end_stroke(s);
        self.auto_down_at = None;
        self.auto_last_at = None;
        self.slop_anchor = None;
    }

    /// Called from the ticker: the focus can change while the pen holds still (no samples). With no
    /// stroke open and the route now the mouse, the tool leaves range; an open stroke is left alone.
    pub(super) fn auto_check_route(&mut self, to_pen: bool) {
        if self.auto_down_at.is_none() && !to_pen && self.pen_proximity {
            let (x, y) = self.last_pen_pos;
            let s = PenSample { x, y, pressure: 0, tilt_x: 0, tilt_y: 0, eraser: false, action: 4 };
            self.ghost_tool_away(&s);
        }
    }

    /// Carry out an announced range exit once the pen has not come back for [`AUTO_EXIT_GRACE`]:
    /// every button and tool on both devices goes up (including a barrel button still held).
    pub(super) fn auto_flush_exit(&mut self) {
        if let Some(at) = self.auto_exit_at {
            if at.elapsed() >= AUTO_EXIT_GRACE && self.auto_down_at.is_none() {
                self.auto_exit_at = None;
                self.force_release_all(Which::Pen);
                self.force_release_all(Which::Pointer);
            }
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
    opts: &crate::config::options::DeviceOptions,
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
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        let mut n: u32 = 0;
        loop {
            tick.tick().await;
            n = n.wrapping_add(1);
            // The idle hide is checked a few times a second, reading the timeout live (before the
            // device lock is taken, as the event path does).
            let opts = crate::config::options::current_options(instance);
            let timeout = if n % 5 == 0 { opts.stylus_idle_hide_timeout } else { 0 };
            let to_pen = route_to_pen(&opts.stylus_art_apps);
            let Ok(mut guard) = uinput.lock() else { continue };
            guard.auto_check_route(to_pen);
            guard.auto_flush_exit();
            if timeout != 0 && guard.ghost_idle_tick(timeout) {
                info!("Pen out of range for {}s — cursor hidden", timeout);
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
            (UInputDevice { file, held: 0 }, path)
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

    // ---- Auto mode (Android): the route is latched per stroke ----

    fn auto(d: &mut InjectDevices, a: u8, x: i32, to_pen: bool) {
        d.auto_pen(&sample(a, x), to_pen, "right", "middle");
    }

    /// The `BTN_LEFT` transitions the kernel would pass on: it drops a key event that repeats the
    /// current value (the lift's belt-and-braces release of an already released button).
    fn left_events(ptr: &[Vec<(u16, u16, i32)>]) -> Vec<i32> {
        let mut v: Vec<i32> = ptr.iter().flatten().filter(|e| e.0 == EV_KEY && e.1 == BTN_LEFT).map(|e| e.2).collect();
        v.dedup();
        v
    }

    #[test]
    fn auto_mouse_route_stroke_is_pointer_only_position_first_then_one_press_held_to_the_lift() {
        // Android: hover, range exit just before the touch-down, down, moves, up. Unknown focus is
        // the same route (`to_pen == false`).
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 9000, false);
        auto(&mut d, 4, 0, false);
        auto(&mut d, 0, 10000, false);
        for i in 1..=4 {
            auto(&mut d, 2, 10000 + i * 1000, false);
        }
        auto(&mut d, 1, 14000, false);
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert!(pen.iter().flatten().next().is_none(), "the tablet device gets nothing at all on the mouse route");
        assert_eq!(left_events(&ptr), vec![1, 0], "one press, no extra press or release during the drag, one release");
        let press = ptr.iter().position(|f| f.contains(&(EV_KEY, BTN_LEFT, 1))).unwrap();
        let at_down = ptr.iter().position(|f| f.contains(&(EV_ABS, ABS_X, 10000))).unwrap();
        assert!(at_down < press, "ABS_X/Y reach the pointer before BTN_LEFT goes down");
        let release = ptr.iter().position(|f| f.contains(&(EV_KEY, BTN_LEFT, 0))).unwrap();
        for i in 1..=4 {
            let m = ptr.iter().position(|f| f.contains(&(EV_ABS, ABS_X, 10000 + i * 1000))).unwrap();
            assert!(press < m && m < release, "move {} is between the press and the release", i);
        }
        assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
    }

    #[test]
    fn auto_art_route_stroke_is_the_tablet_with_real_pressure_and_no_click() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 100, true);
        auto(&mut d, 4, 0, true);
        auto(&mut d, 0, 100, true);
        auto(&mut d, 2, 120, true);
        auto(&mut d, 1, 120, true);
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert!(has(&pen, (EV_KEY, BTN_TOOL_PEN, 1)));
        assert!(has(&pen, (EV_KEY, BTN_TOUCH, 1)) && has(&pen, (EV_KEY, BTN_TOUCH, 0)));
        let pressures: Vec<i32> = pen.iter().flatten().filter(|e| e.1 == ABS_PRESSURE && e.2 > 0).map(|e| e.2).collect();
        assert!(!pressures.is_empty() && pressures.iter().all(|p| *p == 2000), "real pressure, never squeezed: {:?}", pressures);
        assert!(has(&pen, (EV_ABS, ABS_TILT_X, 3)));
        assert!(!has(&ptr, (EV_KEY, BTN_LEFT, 1)), "no left click in an art app");
        assert!(has(&ptr, (EV_ABS, ABS_X, 120)), "the cursor still follows");
        assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
    }

    #[test]
    fn auto_art_route_eraser_end_draws_with_the_rubber_tool() {
        let (mut d, pp, qp) = rig();
        let mut s = sample(0, 100);
        s.eraser = true;
        d.auto_pen(&s, true, "right", "middle");
        let pen = frames(&pp);
        done(&[&pp, &qp]);
        assert!(has(&pen, (EV_KEY, BTN_TOOL_RUBBER, 1)) && has(&pen, (EV_KEY, BTN_TOUCH, 1)));
    }

    #[test]
    fn auto_mouse_route_eraser_end_never_clicks() {
        let (mut d, pp, qp) = rig();
        let mut s = sample(0, 100);
        s.eraser = true;
        d.auto_pen(&s, false, "right", "middle");
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert!(left_events(&ptr).is_empty());
    }

    #[test]
    fn auto_a_focus_change_in_mid_stroke_neither_cuts_nor_moves_the_stroke() {
        // Chrome (mouse) stroke, the focus flips to Krita and back while the button is down.
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 10, false);
        auto(&mut d, 2, 20, true);
        auto(&mut d, 2, 30, false);
        auto(&mut d, 2, 40, true);
        d.auto_check_route(true);
        d.auto_flush_exit();
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1), "still held");
        auto(&mut d, 1, 40, true);
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0]);
        assert!(pen.iter().flatten().next().is_none(), "the stroke stayed on the pointer");

        // And the other way: a Krita stroke stays a tablet stroke when the focus leaves.
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 10, true);
        auto(&mut d, 2, 20, false);
        d.auto_check_route(false);
        assert_eq!(d.dev_state(Which::Pen), DevState::Down(0b1000), "the contact is still down");
        auto(&mut d, 2, 30, false);
        auto(&mut d, 1, 30, false);
        let (pen, ptr) = (frames(&pp), frames(&qp));
        done(&[&pp, &qp]);
        assert!(!has(&ptr, (EV_KEY, BTN_LEFT, 1)));
        let lift = pen.iter().position(|f| f.contains(&(EV_KEY, BTN_TOUCH, 0))).unwrap();
        let last_move = pen.iter().rposition(|f| f.contains(&(EV_ABS, ABS_X, 30))).unwrap();
        assert!(last_move <= lift, "every move before the lift reached the tablet");
        assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
    }

    #[test]
    fn auto_a_repeated_down_in_mid_drag_is_a_move_not_a_release_and_press() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 1000, false);
        auto(&mut d, 0, 1000, false);
        auto(&mut d, 2, 2000, false);
        auto(&mut d, 0, 3000, false); // a DOWN again, long after the first? no: while the drag runs
        auto(&mut d, 2, 4000, false);
        auto(&mut d, 1, 4000, false);
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0], "the hold is never interrupted");
        assert!(has(&ptr, (EV_ABS, ABS_X, 3000)), "the repeated DOWN's position still moves the cursor");
    }

    #[test]
    fn auto_a_down_after_a_lost_up_closes_the_old_stroke_first() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 10, false);
        std::thread::sleep(AUTO_STALE_STROKE + Duration::from_millis(30));
        auto(&mut d, 0, 90, false); // the UP in between never arrived
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0, 1]);
    }

    #[test]
    fn auto_a_move_whose_down_was_lost_starts_the_stroke() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 10, false);
        auto(&mut d, 2, 20, false); // no DOWN ever arrived
        auto(&mut d, 2, 30, false);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
        auto(&mut d, 1, 30, false);
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0]);
    }

    #[test]
    fn auto_a_straggler_move_right_after_the_lift_does_not_press_again() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 10, false);
        auto(&mut d, 1, 10, false);
        auto(&mut d, 2, 15, false); // arrived late
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0]);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
    }

    #[test]
    fn auto_a_range_exit_inside_a_stroke_does_not_let_go() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 10, false);
        auto(&mut d, 4, 0, false);
        std::thread::sleep(AUTO_EXIT_GRACE + Duration::from_millis(20));
        d.auto_flush_exit();
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
        auto(&mut d, 2, 20, false);
        auto(&mut d, 1, 20, false);
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0]);
    }

    #[test]
    fn auto_range_exit_before_the_tip_keeps_a_held_barrel_button() {
        // Android: barrel held, hover exit, DOWN within a few ms: a right-drag.
        let (mut d, pp, qp) = rig();
        auto(&mut d, 5, 0, false);
        auto(&mut d, 4, 0, false);
        d.auto_flush_exit();
        auto(&mut d, 0, 10, false);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b11));
        done(&[&pp, &qp]);
    }

    #[test]
    fn auto_range_exit_that_lasts_leaves_both_devices_idle_including_the_right_button() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 5, 0, false); // barrel held
        auto(&mut d, 3, 10, true);
        auto(&mut d, 4, 0, true);
        std::thread::sleep(AUTO_EXIT_GRACE + Duration::from_millis(20));
        d.auto_flush_exit();
        done(&[&pp, &qp]);
        assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
        assert!(d.auto_exit_at.is_none());
    }

    #[test]
    fn auto_pen_up_lets_go_on_both_devices() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 10, false);
        auto(&mut d, 1, 10, false);
        auto(&mut d, 0, 20, true);
        auto(&mut d, 1, 20, true);
        done(&[&pp, &qp]);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
        assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
        assert_eq!(d.pen_mouse_buttons, 0);
        assert!(!d.pen_proximity);
    }

    #[test]
    fn auto_hover_follows_the_route_the_tool_in_range_only_on_the_art_route() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 100, true);
        assert!(d.pen_proximity);
        auto(&mut d, 3, 110, false);
        assert!(!d.pen_proximity, "left the art app: the tool leaves range");
        auto(&mut d, 3, 120, true);
        assert!(d.pen_proximity);
        d.auto_check_route(false); // the focus moved with the pen held still
        assert!(!d.pen_proximity);
        done(&[&pp, &qp]);
    }

    #[test]
    fn auto_barrel_button_follows_the_route() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 5, 0, false);
        assert_eq!(d.pen_mouse_buttons & 2, 2, "mouse route: right button");
        auto(&mut d, 6, 0, false);
        assert_eq!(d.pen_mouse_buttons, 0);
        auto(&mut d, 5, 0, true);
        let pen = frames(&pp);
        done(&[&pp, &qp]);
        assert!(has(&pen, (EV_KEY, BTN_STYLUS, 1)), "art route: the tablet's barrel button");
    }

    // ---- Click slop (mouse route) ----

    fn xs(f: &[Vec<(u16, u16, i32)>]) -> Vec<i32> {
        f.iter().flat_map(|fr| fr.iter().filter(|e| e.1 == ABS_X).map(|e| e.2)).collect()
    }

    #[test]
    fn slop_a_wobble_inside_the_zone_never_reaches_the_pointer_and_the_lift_releases_at_the_press_point() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 5000, false);
        auto(&mut d, 0, 5000, false);
        let before = xs(&frames(&qp)).len();
        for dx in [60, -80, 120, -100, 150] {
            auto(&mut d, 2, 5000 + dx, false); // under 1 mm
        }
        auto(&mut d, 1, 5100, false);
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(xs(&ptr).len(), before, "no pointer motion while inside the slop, not even at the lift");
        assert_eq!(left_events(&ptr), vec![1, 0], "a clean click");
    }

    #[test]
    fn slop_leaving_the_zone_releases_the_pointer_for_the_rest_of_the_stroke() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 5000, false);
        auto(&mut d, 2, 5100, false); // inside
        auto(&mut d, 2, 8000, false); // ~12 mm: out
        auto(&mut d, 2, 5050, false); // back near the start: it is a drag now, so it follows
        auto(&mut d, 1, 5050, false);
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        let x = xs(&ptr);
        assert!(x.contains(&8000) && x.last() == Some(&5050), "{:?}", x);
        assert!(!x.contains(&5100), "the wobble inside the zone was held");
    }

    #[test]
    fn slop_does_not_apply_on_the_art_route() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 5000, true);
        auto(&mut d, 2, 5060, true);
        let ptr = frames(&qp);
        let pen = frames(&pp);
        done(&[&pp, &qp]);
        assert!(xs(&ptr).contains(&5060), "the cursor follows every sample");
        assert!(has(&pen, (EV_ABS, ABS_X, 5060)), "and so does the tablet");
    }

    #[test]
    fn slop_is_cleared_when_the_stroke_ends() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 0, 5000, false);
        auto(&mut d, 1, 5000, false);
        assert!(d.slop_anchor.is_none());
        auto(&mut d, 3, 5030, false);
        assert!(xs(&frames(&qp)).contains(&5030), "hovering follows at once");
        done(&[&pp, &qp]);
    }

    // ---- Resting pen: parked hover writes nothing ----

    #[test]
    fn auto_a_resting_pen_is_parked_and_stops_touching_the_cursor() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 5000, false);
        auto(&mut d, 3, 5010, false); // noise
        std::thread::sleep(d.still_cfg.park_after + Duration::from_millis(40));
        auto(&mut d, 3, 5005, false); // parks here
        let writes_when_parked = frames(&qp).len();
        for i in 0..20 {
            auto(&mut d, 3, 5000 + (i % 7), false); // a parked pen's noise
        }
        let after = frames(&qp).len();
        done(&[&pp, &qp]);
        assert!(writes_when_parked >= 1, "it was followed before parking");
        assert_eq!(after, writes_when_parked, "nothing more is written to the pointer while parked");
    }

    #[test]
    fn auto_a_parked_pen_on_the_art_route_leaves_range_and_wakes_on_clear_movement() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 5000, true);
        assert!(d.pen_proximity);
        std::thread::sleep(d.still_cfg.park_after + Duration::from_millis(40));
        auto(&mut d, 3, 5002, true);
        assert!(!d.pen_proximity, "parked: the tablet tool is out of range and cannot own the cursor");
        auto(&mut d, 3, 5003, true);
        assert!(!d.pen_proximity);
        auto(&mut d, 3, 5000 + 4000, true); // ~ 16 mm away
        assert!(d.pen_proximity, "woken by clear movement: the tool is back");
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert!(has(&ptr, (EV_ABS, ABS_X, 9000)));
    }

    #[test]
    fn auto_touching_down_works_from_a_parked_pen() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 3, 5000, false);
        std::thread::sleep(d.still_cfg.park_after + Duration::from_millis(40));
        auto(&mut d, 3, 5001, false);
        auto(&mut d, 4, 0, false);
        auto(&mut d, 0, 5001, false);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
        auto(&mut d, 1, 5001, false);
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert_eq!(left_events(&ptr), vec![1, 0]);
    }

    // ---- Tracked state and forced releases ----

    #[test]
    fn device_state_follows_what_the_device_holds() {
        let (mut d, pp, qp) = rig();
        assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
        auto(&mut d, 3, 10, true);
        assert_eq!(d.dev_state(Which::Pen), DevState::Hovering);
        auto(&mut d, 0, 10, true);
        assert_eq!(d.dev_state(Which::Pen), DevState::Down(0b1000), "tip contact");
        auto(&mut d, 1, 10, true);
        assert_eq!(d.dev_state(Which::Pen), DevState::Idle);
        auto(&mut d, 0, 20, false);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b1));
        done(&[&pp, &qp]);
    }

    #[test]
    fn force_release_all_lets_go_of_everything_a_device_holds() {
        let (mut d, pp, qp) = rig();
        auto(&mut d, 5, 0, false); // barrel -> right button on the pointer
        auto(&mut d, 0, 10, false);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Down(0b11));
        d.force_release_all(Which::Pointer);
        assert_eq!(d.dev_state(Which::Pointer), DevState::Idle);
        assert!(d.auto_down_at.is_none(), "the stroke is closed with it");
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        assert!(has(&ptr, (EV_KEY, BTN_LEFT, 0)) && has(&ptr, (EV_KEY, BTN_RIGHT, 0)));
        assert_eq!(d.pen_mouse_buttons, 0);
        assert_eq!(d.tip, Tip::None);
    }

    #[test]
    fn a_second_left_down_on_the_pointer_device_is_written_as_it_comes() {
        // The device layer no longer rewrites a press (an inserted release would break a hold).
        let (mut d, pp, qp) = rig();
        {
            let dev = d.pointer.as_mut().unwrap();
            dev.emit(EV_KEY, BTN_LEFT, 1).unwrap();
            dev.syn().unwrap();
            dev.emit(EV_KEY, BTN_LEFT, 1).unwrap();
            dev.syn().unwrap();
        }
        let ptr = frames(&qp);
        done(&[&pp, &qp]);
        let raw: Vec<i32> = ptr.iter().flatten().filter(|e| e.1 == BTN_LEFT).map(|e| e.2).collect();
        assert_eq!(raw, vec![1, 1]);
    }

    #[test]
    fn a_lift_that_does_not_name_the_eraser_still_releases_the_rubber_tool() {
        // Plain tablet path: DOWN with the eraser end, then an UP sample (a cancel) without it.
        let (mut d, pp, qp) = rig();
        {
            let pen = d.pen.as_mut().unwrap();
            pen.inject_pen(10, 10, 500, 0, 0, 0, true).unwrap();
            assert_ne!(pen.held & key_bit(BTN_TOOL_RUBBER).unwrap(), 0);
            pen.inject_pen(10, 10, 0, 0, 0, 1, false).unwrap();
            assert_eq!(pen.state(), DevState::Idle, "no tool key left down");
        }
        done(&[&pp, &qp]);
    }
}
