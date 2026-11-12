//! The Android "auto" pen style: one pen that is a mouse or a graphics tablet *per stroke*,
//! depending on the focused app. Mouse route = pointer only with click slop and a held-back press
//! (so a long press can show the pen menu instead of clicking); art route = tablet contact with
//! real pressure; the route is latched at pen-down, and a resting pen is parked so it
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

/// Where the pen menu stands for the open mouse-route stroke. The left-button press is held back
/// (`press_deferred`) until it is known whether this is a click, a drag or a long press, so a long
/// press never clicks the text it is meant to act on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::input) enum Hold {
    /// Not held long enough yet.
    #[default]
    None,
    /// Held long enough: the runner is deciding whether a menu applies here.
    Pending,
    /// The menu is on screen: the rest of this touch is swallowed (no click, no drag).
    Shown,
    /// No menu applies (or the pen moved on): the press went out and the stroke is an ordinary one.
    Declined,
}

impl InjectDevices {
    /// **Auto mode** (Android pen, `stylus_style = "auto"`): the pen is a mouse or a graphics tablet,
    /// per stroke, depending on the focused app (`to_pen`).
    ///
    /// * The pointer follows every sample (X/Y first, then the button), so the cursor is always
    ///   visible and a click lands where the pen is.
    /// * **Mouse route** (not an art app, or unknown): the tip is `BTN_LEFT` on the pointer, held for
    ///   the whole stroke, but its press is *deferred*: sent when the pen leaves the click slop (a
    ///   drag, the press goes out at the spot it landed), at the lift (a tap) or when a long press
    ///   is declined. Moves go to the pointer only. The tablet device gets nothing, the tool
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
        if matches!(s.action, 0..=3) {
            // The pen is (still) there: an announced exit is void.
            self.auto_exit_at = None;
        }
        if s.action != 3 {
            // Anything but a hover (touch, lift, range exit) ends a parked spell.
            self.hover_filter.reset();
        }
        // A new touch (or a move that starts one) first lets go of the previous tap's button.
        if matches!(s.action, 0 | 2) {
            self.auto_release_click_now();
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
                self.menu_track_lift(open);
                // A lift inside the click slop releases where the tip landed.
                if !(open && self.slop_anchor.is_some()) {
                    self.ghost_place_pointer(s.x, s.y);
                }
                let mut tap = false;
                if open && self.tip == Tip::Pointer && self.press_deferred {
                    match self.hold {
                        // A long press with the menu up: no click, the selection stays.
                        Hold::Shown => {}
                        // The runner is still deciding: the tap is delivered or dropped when it answers.
                        Hold::Pending => self.pending_click = self.slop_anchor,
                        // A plain tap: the press goes out now; the button stays down for the click
                        // hold time (like a real click) and is released by `auto_flush_click`.
                        _ => {
                            self.auto_press_now();
                            tap = true;
                        }
                    }
                }
                let keep_down = tap && !self.still_cfg.click_hold.is_zero();
                if keep_down {
                    // `ghost_end_stroke` would release the button now; leave it to the timer.
                    self.tip = Tip::None;
                    self.click_release_at = Some(Instant::now() + self.still_cfg.click_hold);
                }
                self.auto_end_stroke(s);
                // Whatever the lift did not manage to let go of, let go of now (only what is held:
                // the tablet device is not written to when it is idle).
                if !keep_down {
                    self.auto_let_go_left();
                }
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
            }
            5 | 6 if self.tip == Tip::Pen || (!open && to_pen) => self.ghost_tablet(s, s.action),
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

    /// Start a stroke on the route `to_pen` says (latched until the lift). The pointer is already
    /// at the pen, so the button goes down after the position.
    fn auto_begin(&mut self, s: &PenSample, to_pen: bool) {
        // The previous tap's button must be up before anything new starts.
        self.auto_release_click_now();
        self.tip = if to_pen {
            Tip::Pen
        } else if s.eraser {
            Tip::None
        } else {
            Tip::Pointer
        };
        let now = Instant::now();
        self.auto_down_at = Some(now);
        if self.menu_armed {
            self.menu_armed = false;
            self.menu_second = true;
        }
        self.auto_last_at = Some(now);
        self.slop_anchor = (self.tip == Tip::Pointer).then_some((s.x, s.y));
        match self.tip {
            // Tool, X/Y, tilt, then BTN_TOUCH with the real pressure (see `inject_pen`).
            Tip::Pen => self.ghost_tablet(s, 0),
            Tip::Pointer => {
                // The press itself waits (see `Hold`), unless the menu is switched off.
                self.ghost_tool_away(s);
                self.press_deferred = self.menu_enabled;
                self.hold = Hold::None;
                if !self.menu_enabled {
                    self.ghost_pointer_button(BTN_LEFT, 1, true);
                }
            }
            Tip::None => self.ghost_tool_away(s),
        }
        self.last_pen_pos = (s.x, s.y);
    }

    /// Move the pointer for a sample of the open stroke: held at the press point while the pen stays
    /// inside the click slop, following normally from the moment it leaves it (and never held
    /// again for this stroke).
    fn auto_stroke_pointer(&mut self, s: &PenSample) {
        // The menu is up: this touch is over as far as the app is concerned.
        if self.hold == Hold::Shown {
            return;
        }
        if let Some(anchor) = self.slop_anchor {
            let inside = super::stillness::dist_mm(anchor, (s.x, s.y), self.tablet_mm)
                <= self.still_cfg.click_slop_mm;
            if inside {
                return;
            }
            self.slop_anchor = None;
            // A drag: the held-back press goes out where the tip landed, then the pointer follows.
            if self.hold == Hold::Pending {
                self.hold = Hold::Declined;
            }
            self.auto_press_now();
        }
        self.ghost_place_pointer(s.x, s.y);
    }

    /// Release a tap's left button when its hold time is up. Called by the 10 ms ticker.
    pub(in crate::input) fn auto_flush_click(&mut self) {
        if self.click_release_at.is_some_and(|t| Instant::now() >= t) {
            self.auto_release_click_now();
        }
    }

    /// Release a tap's left button now, if it is still down.
    fn auto_release_click_now(&mut self) {
        if self.click_release_at.take().is_some() {
            self.ghost_pointer_button(BTN_LEFT, 1, false);
        }
    }

    /// Send the left-button press that was held back, if it still is. The pointer is at the spot the
    /// tip landed (it was held there), so the press goes out where the user touched.
    fn auto_press_now(&mut self) {
        if self.press_deferred {
            self.press_deferred = false;
            self.ghost_pointer_button(BTN_LEFT, 1, true);
        }
    }

    /// Called from the ticker: a tip held still, inside the click slop, for the long-press time asks
    /// the menu runner whether a menu applies here. Without a runner the press is simply delivered.
    pub(in crate::input) fn auto_hold_check(&mut self) {
        if self.tip == Tip::Pointer
            && self.press_deferred
            && self.hold == Hold::None
            && self
                .auto_down_at
                .is_some_and(|t| t.elapsed() >= self.still_cfg.long_press)
        {
            self.hold = Hold::Pending;
            if !crate::input::menu::request(&self.menu_tx) {
                self.auto_menu_resolved(false);
            }
        }
    }

    /// Follow the touches around an open menu: the touch that opened it, then the next one. When the
    /// next touch lifts, the menu is closed shortly after unless that touch chose a button (the
    /// runner sees the choice first). The tap itself still reaches the app as a plain click.
    fn menu_track_lift(&mut self, open: bool) {
        if !self.menu_open || !open {
            return;
        }
        if self.hold == Hold::Shown && !self.menu_second {
            self.menu_armed = true;
        } else if self.menu_second {
            self.menu_second = false;
            self.menu_dismiss_at = Some(Instant::now() + Duration::from_millis(150));
        }
    }

    /// The menu was closed (chosen, dismissed or timed out): forget its touches.
    pub(in crate::input) fn menu_closed(&mut self) {
        self.menu_open = false;
        self.menu_armed = false;
        self.menu_second = false;
        self.menu_dismiss_at = None;
    }

    /// Whether the touch after the menu opened has ended without a choice, so the menu should close.
    pub(in crate::input) fn menu_dismiss_due(&self) -> bool {
        self.menu_dismiss_at.is_some_and(|t| Instant::now() >= t)
    }

    /// Whether a menu question is open for the current stroke.
    pub(in crate::input) fn hold_pending(&self) -> bool {
        self.hold == Hold::Pending && self.auto_down_at.is_some()
    }

    /// The runner's answer: `shown` = a menu is on screen. Shown swallows the rest of the touch;
    /// declined delivers the press that was held back (late, if the pen has already lifted).
    pub(in crate::input) fn auto_menu_resolved(&mut self, shown: bool) {
        if let Some(p) = self.pending_click.take() {
            if !shown {
                self.ghost_place_pointer(p.0, p.1);
                self.ghost_pointer_button(BTN_LEFT, 1, true);
                if self.still_cfg.click_hold.is_zero() {
                    self.ghost_pointer_button(BTN_LEFT, 1, false);
                } else {
                    self.click_release_at = Some(Instant::now() + self.still_cfg.click_hold);
                }
            }
            return;
        }
        if self.hold_pending() {
            if shown {
                self.hold = Hold::Shown;
                self.menu_open = true;
                self.menu_armed = false;
                self.menu_second = false;
                self.menu_dismiss_at = None;
            } else {
                self.hold = Hold::Declined;
                self.auto_press_now();
            }
        }
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
        self.press_deferred = false;
        self.hold = Hold::None;
    }

    /// Called from the ticker: the focus can change while the pen holds still (no samples). With no
    /// stroke open and the route now the mouse, the tool leaves range; an open stroke is left alone.
    pub(in crate::input) fn auto_check_route(&mut self, to_pen: bool) {
        if self.auto_down_at.is_none()
            && !to_pen
            && (self.pen_proximity || self.dev_state(Which::Pen) != DevState::Idle)
        {
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
