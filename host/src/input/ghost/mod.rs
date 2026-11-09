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
//!
//! Layout: `route` (which app the pen draws into, per-app overrides), `pointer` (the two-device
//! injection used by physical tablets), `auto` (the Android "auto" style: per-stroke routing, click
//! slop, hover parking), `background` (the focus/idle threads), `control` (request files) and
//! `tests`.

use super::*;

mod auto;
mod background;
mod control;
mod pointer;
mod route;
#[cfg(test)]
mod tests;

pub(in crate::input) use crate::desktop::session_is_x11;
pub(in crate::input) use background::{spawn_idle_task, start_focus_watcher};
pub use control::{dump_file, dump_request_file, toggle_file};
pub(in crate::input) use pointer::apply_physical;
pub(in crate::input) use route::route_to_pen;

/// One pen sample in the injector's units: position 0..=COORD_MAX over the pen's output, pressure
/// 0..=PEN_PRESSURE_MAX, tilt in degrees.
///
/// `action` is the pen protocol's: 0 down, 1 up, 2 move (touching), 3 hover, 4 left range,
/// 5/6 first side button down/up, 7/8 second side button down/up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::input) struct PenSample {
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
pub(in crate::input) enum Tip {
    #[default]
    None,
    Pen,
    Pointer,
}
