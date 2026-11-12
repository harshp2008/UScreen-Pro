//! The two-device pen injection ("ghost tracking"): every sample also moves `UScreen Pointer`, so
//! the desktop shows one ordinary cursor under the pen. This file holds the shared primitives
//! (place the pointer, press a button, write a tablet frame, release everything) and `ghost_pen`,
//! the routing used by physical drawing tablets. The Android "auto" style builds on the same
//! primitives in `auto.rs`.

use super::route::route_to_pen;
use super::*;

impl InjectDevices {
    /// Put the absolute pointer under the pen (no button): the visible cursor.
    ///
    /// While the tablet tool is in range its events hide the pointer unless a pointer-class event
    /// came in the last 300 ms, but the kernel drops an absolute event whose value did not change.
    /// A pen held still (a real tablet still reports tilt and pressure noise) would therefore let
    /// the cursor vanish; so when the tool is up and the position repeats, it is written one unit
    /// (1/65535 of the desktop, far below a pixel) to the side and back.
    pub(super) fn ghost_place_pointer(&mut self, x: i32, y: i32) {
        // A tap's button is still down for its click hold time: the cursor must not move yet, or the
        // app would see a tiny drag.
        if self.click_release_at.is_some() {
            return;
        }
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

    /// Press or release a pointer button and keep the pen's held-button bookkeeping.
    pub(super) fn ghost_pointer_button(&mut self, code: u16, bit: u8, down: bool) {
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
    pub(in crate::input) fn ghost_pen(
        &mut self,
        s: &PenSample,
        to_pen: bool,
        primary: &str,
        secondary: &str,
    ) {
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
            }
            5 | 6 if to_pen => self.ghost_tablet(s, s.action),
            5..=8 => {
                let name = if s.action <= 6 { primary } else { secondary };
                let down = s.action == 5 || s.action == 7;
                if self.run_button_action(name, down) {
                    return;
                }
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
    pub(super) fn ghost_tool(&mut self, s: &PenSample, to_pen: bool) {
        if to_pen {
            self.ghost_tablet(s, 3);
        } else {
            self.ghost_tool_away(s);
        }
    }

    /// Take the tablet tool out of range if it is in range (a route flip, or the start of a click).
    pub(super) fn ghost_tool_away(&mut self, s: &PenSample) {
        // Also when the bookkeeping says "out" but the device still holds a tool key: a stuck tool
        // leaves a second, stale cursor on screen.
        if self.pen_proximity || self.dev_state(Which::Pen) != DevState::Idle {
            self.ghost_tablet(s, 4);
        }
    }

    /// Write a pen frame for `action` to the tablet device, keeping the proximity bookkeeping.
    pub(super) fn ghost_tablet(&mut self, s: &PenSample, action: u8) {
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
    pub(super) fn ghost_end_stroke(&mut self, s: &PenSample) {
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
    pub(in crate::input) fn ghost_panic_release(&mut self, everything: bool) {
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
}

/// A physical tablet's sample: always ghost tracking (the tablet exists to be a pen with a visible
/// cursor), whatever the Android pen's `stylus_style` is, unless X11 or no pointer device.
pub(in crate::input) fn apply_physical(
    guard: &mut InjectDevices,
    opts: &crate::config::options::DeviceOptions,
    s: &PenSample,
) {
    if guard.has_pointer() && !session_is_x11() {
        let to_pen = route_to_pen(&opts.stylus_art_apps);
        guard.ghost_pen(
            s,
            to_pen,
            &opts.stylus_button_primary,
            &opts.stylus_button_secondary,
        );
    } else {
        super::events::apply_pen(guard, opts, s);
    }
}
