//! The Android "auto" pen style: one pen that is a mouse or a graphics tablet *per stroke*,
//! depending on the focused app. Mouse route = pointer only with click slop; art route = tablet
//! contact with real pressure; the route is latched at pen-down, and a resting pen is parked so it
//! stops taking the cursor from other devices.

use super::*;
use crate::input::recorder::record;
use crate::input::stillness::HoverVerdict;
use std::time::{Duration, Instant};

/// How long an announced range exit waits for the pen to come back before the tool really leaves
/// range (Android's hover exit precedes the touch-down by a few milliseconds).
pub(super) const AUTO_EXIT_GRACE: Duration = Duration::from_millis(150);

/// A DOWN while a stroke is open and a sample arrived this recently is the drag going on (a
/// repeated DOWN), not a new press. Older than this, the stroke lost its UP and is closed first.
pub(super) const AUTO_STALE_STROKE: Duration = Duration::from_millis(500);

/// A MOVE this soon after a lift is a straggler that overtook the UP, not a stroke that lost its
/// DOWN: it must not press again.
pub(super) const AUTO_LIFT_QUIET: Duration = Duration::from_millis(100);

impl InjectDevices {
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
    pub(in crate::input) fn auto_pen(
        &mut self,
        s: &PenSample,
        to_pen: bool,
        primary: &str,
        secondary: &str,
    ) {
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
            record(
                s,
                to_pen,
                match s.action {
                    0 => "down",
                    1 => "up",
                    2 => "move",
                    4 => "exit",
                    _ => "button",
                },
            );
        }
        match s.action {
            0 => {
                if open {
                    if self
                        .auto_last_at
                        .is_some_and(|t| t.elapsed() < AUTO_STALE_STROKE)
                    {
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
                    if !s.eraser
                        && self
                            .auto_up_at
                            .is_none_or(|t| t.elapsed() >= AUTO_LIFT_QUIET)
                    {
                        warn!(
                            "Pen MOVE with no stroke open (its DOWN was lost): starting the stroke"
                        );
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
                let verdict = self.hover_filter.sample(
                    s.x,
                    s.y,
                    Instant::now(),
                    self.tablet_mm,
                    &self.still_cfg,
                );
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
                // A tip that stayed down, inside the click slop, for the long-press time is a long
                // press: once the button is up, ask for the pen menu.
                let long_press = open
                    && self.tip == Tip::Pointer
                    && self.slop_anchor.is_some()
                    && self
                        .auto_down_at
                        .is_some_and(|t| t.elapsed() >= self.still_cfg.long_press);
                self.auto_end_stroke(s);
                if long_press {
                    crate::input::menu::request(&self.menu_tx);
                }
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
            let inside = super::stillness::dist_mm(anchor, (s.x, s.y), self.tablet_mm)
                <= self.still_cfg.click_slop_mm;
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
    pub(in crate::input) fn auto_check_route(&mut self, to_pen: bool) {
        if self.auto_down_at.is_none() && !to_pen && self.pen_proximity {
            let (x, y) = self.last_pen_pos;
            let s = PenSample {
                x,
                y,
                pressure: 0,
                tilt_x: 0,
                tilt_y: 0,
                eraser: false,
                action: 4,
            };
            self.ghost_tool_away(&s);
        }
    }

    /// Carry out an announced range exit once the pen has not come back for [`AUTO_EXIT_GRACE`]:
    /// every button and tool on both devices goes up (including a barrel button still held).
    pub(in crate::input) fn auto_flush_exit(&mut self) {
        if let Some(at) = self.auto_exit_at {
            if at.elapsed() >= AUTO_EXIT_GRACE && self.auto_down_at.is_none() {
                self.auto_exit_at = None;
                self.force_release_all(Which::Pen);
                self.force_release_all(Which::Pointer);
            }
        }
    }
}
