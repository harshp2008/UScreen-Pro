//! `InjectDevices`: the set of virtual devices behind one tablet connection plus all the state the
//! input router keeps next to them (held buttons, pen stroke state, hover filter, pinch). Creates
//! devices on demand, maps the pen onto the desktop, and releases everything on disconnect.

use super::*;

/// Which virtual device(s) [`InjectDevices::force_release_all`] acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::input) enum Which {
    /// `UScreen Pen`.
    Pen,
    /// `UScreen Pointer` (the absolute mouse).
    Pointer,
    /// The touchpad: the relative mouse and the multi-touch pad.
    Touchpad,
    /// Everything, including the touchscreen.
    All,
}

/// The pair of virtual input devices backing one tablet connection.
pub(in crate::input) struct InjectDevices {
    pub(in crate::input) touch: Option<UInputDevice>,
    pub(in crate::input) pen: Option<UInputDevice>,
    /// Takes over the cursor when the pen leaves proximity, so it stays where
    /// the user last pointed instead of vanishing.
    pub(in crate::input) pointer: Option<UInputDevice>,
    /// Relative mouse for touchpad mode, made on first use.
    pub(in crate::input) mouse: Option<UInputDevice>,
    pub(in crate::input) mouse_name: String,
    pub(in crate::input) mouse_product: u16,
    /// The multi-touch touchpad native gestures are replayed on. Exists only while the input mode
    /// is Touchpad and native gestures are in use.
    pub(in crate::input) pad: Option<UInputDevice>,
    pub(in crate::input) pad_name: String,
    pub(in crate::input) pad_product: u16,
    pub(in crate::input) pad_state: PadState,
    /// Keyboard for key combinations, made on first use.
    pub(in crate::input) keyboard: Option<UInputDevice>,
    pub(in crate::input) keyboard_name: String,
    pub(in crate::input) keyboard_product: u16,
    /// Stylus mode: the relative mouse must not exist.
    pub(in crate::input) stylus_mode: bool,
    /// Mouse buttons held down (bit 0 left, 1 right, 2 middle), so a dropped connection releases
    /// them.
    pub(in crate::input) mouse_buttons: u8,
    /// Scroll left over after whole notches were sent, per axis.
    pub(in crate::input) scroll_rest: (i32, i32),
    /// Sub-pixel remainder of pointer movement.
    pub(in crate::input) rel_rest: (f64, f64),
    pub(in crate::input) last_pen_pos: (i32, i32),
    /// Where the output the pen addresses sits on the desktop, as fractions
    /// of the whole desktop: (x, y, width, height).
    ///
    /// The pen is a tablet tool and KWin puts it inside the output named in
    /// `outputName`. The plain absolute pointer is not a tablet tool, and
    /// KWin spreads it across the entire desktop whatever that property
    /// says — so parking it at the pen's own coordinates dropped the cursor
    /// somewhere else entirely, further left and lower the larger the rest
    /// of the desktop was (#18). Converting here puts it where the pen was.
    pub(in crate::input) pointer_area: Option<(f64, f64, f64, f64)>,
    /// The desktop in physical pixels: the absolute pointer is snapped to whole pixels of this grid,
    /// because fractional positions leave stale "ghost" cursor images behind with software cursors.
    pub(in crate::input) snap_grid: Option<(f64, f64)>,
    /// Bitmask of MT slots that currently have an active tracking ID
    /// (DOWN received, no matching UP yet). Bit N → slot N, up to slot 15.
    pub(in crate::input) active_slots: u16,
    pub(in crate::input) pen_proximity: bool,
    /// S Pen side button currently held (BTN_STYLUS). Tracked so a held
    /// button is released cleanly if the connection drops.
    pub(in crate::input) pen_button: bool,
    /// Buttons the pen holds on the absolute pointer in `mouse` style (bit 0 left, 1 right,
    /// 2 middle), so a dropped connection lets go of them.
    pub(in crate::input) pen_mouse_buttons: u8,
    /// Ghost tracking: where the current stroke's tip goes (fixed at pen-down).
    pub(in crate::input) tip: Tip,
    /// Ghost tracking: since when the pen has been out of range, for the idle hide.
    pub(in crate::input) ghost_out_since: Option<std::time::Instant>,
    /// Ghost tracking: the cursor has been hidden for this out-of-range spell.
    pub(in crate::input) ghost_hidden: bool,
    /// Auto mode: when the tablet tool's range exit was announced; it is only carried out after a
    /// short grace (see `auto_flush_exit`), because Android sends a hover exit right before every
    /// touch-down and the tool must not leave and re-enter range around the click.
    pub(in crate::input) auto_exit_at: Option<std::time::Instant>,
    /// Auto mode: when the open stroke's DOWN arrived (`Some` while a stroke is open).
    pub(in crate::input) auto_down_at: Option<std::time::Instant>,
    /// Auto mode: when the open stroke last got a sample.
    pub(in crate::input) auto_last_at: Option<std::time::Instant>,
    /// Auto mode: when the last stroke was lifted.
    pub(in crate::input) auto_up_at: Option<std::time::Instant>,
    /// A tap's left button is down and is due to be released at this time (see `StillCfg::click_hold`).
    pub(in crate::input) click_release_at: Option<std::time::Instant>,
    /// The pen menu is switched on (config `pen_menu`); off restores plain, immediate clicks.
    pub(in crate::input) menu_enabled: bool,
    /// Auto mode, mouse route: the left-button press of the open stroke has not been sent yet (it is
    /// held back until the pen moves out of the click slop, lifts, or the menu question is answered).
    pub(in crate::input) press_deferred: bool,
    /// Auto mode: where the pen menu stands for the open stroke.
    pub(in crate::input) hold: crate::input::ghost::Hold,
    /// A tap that ended while the menu question was still open: delivered (or dropped) when it is answered.
    pub(in crate::input) pending_click: Option<(i32, i32)>,
    /// A pen menu is on screen. `menu_armed`: the touch that opened it has lifted; `menu_second`: the
    /// next touch is down; `menu_dismiss_at`: that touch lifted without choosing, so close the menu then.
    pub(in crate::input) menu_open: bool,
    pub(in crate::input) menu_armed: bool,
    pub(in crate::input) menu_second: bool,
    pub(in crate::input) menu_dismiss_at: Option<std::time::Instant>,
    /// Where long-press requests for the pen menu go (set when the menu runner starts).
    pub(in crate::input) menu_tx: Option<std::sync::mpsc::Sender<()>>,
    /// Auto mode, mouse route: where the tip landed, while the pointer is held there (click slop).
    pub(in crate::input) slop_anchor: Option<(i32, i32)>,
    /// Auto mode: parks a pen that rests near the glass, so it stops taking the cursor.
    pub(in crate::input) hover_filter: super::stillness::HoverFilter,
    /// Thresholds of the hover filter.
    pub(in crate::input) still_cfg: super::stillness::StillCfg,
    /// The tablet's physical size in millimetres (width, height), for distances.
    pub(in crate::input) tablet_mm: (f64, f64),
    /// Ghost tracking: the last position written to the pointer (to nudge a repeat, see `ghost_place_pointer`).
    pub(in crate::input) ghost_last_written: (i32, i32),
    /// Ctrl is held on the keyboard for a pinch in progress, and when the last pinch step came.
    pub(in crate::input) pinch_since: Option<std::time::Instant>,
}

impl InjectDevices {
    /// No devices yet: pen and touch are absent, the mouse and keyboard are made on first use.
    pub(in crate::input) fn headless() -> Self {
        Self {
            touch: None,
            pen: None,
            pointer: None,
            mouse: None,
            mouse_name: MOUSE_DEVICE_NAME.into(),
            mouse_product: PRODUCT_MOUSE,
            pad: None,
            pad_name: TOUCHPAD_DEVICE_NAME.into(),
            pad_product: PRODUCT_TOUCHPAD,
            pad_state: PadState::new(),
            keyboard: None,
            keyboard_name: KEYBOARD_DEVICE_NAME.into(),
            keyboard_product: PRODUCT_KEYBOARD,
            stylus_mode: false,
            mouse_buttons: 0,
            scroll_rest: (0, 0),
            rel_rest: (0.0, 0.0),
            last_pen_pos: (0, 0),
            pointer_area: None,
            snap_grid: None,
            active_slots: 0,
            pen_proximity: false,
            pen_button: false,
            pen_mouse_buttons: 0,
            tip: Tip::None,
            ghost_out_since: None,
            ghost_hidden: false,
            auto_exit_at: None,
            auto_down_at: None,
            auto_last_at: None,
            auto_up_at: None,
            click_release_at: None,
            menu_enabled: true,
            press_deferred: false,
            hold: Default::default(),
            pending_click: None,
            menu_open: false,
            menu_armed: false,
            menu_second: false,
            menu_dismiss_at: None,
            menu_tx: None,
            slop_anchor: None,
            hover_filter: Default::default(),
            still_cfg: Default::default(),
            tablet_mm: (266.0, 166.0),
            ghost_last_written: (-1, -1),
            pinch_since: None,
        }
    }

    /// Release all active contacts cleanly before the connection closes.
    /// Without this, a stuck MT slot or a pen left in proximity causes
    /// the next connection to inherit phantom input events.
    pub(in crate::input) fn release_all(&mut self) {
        if let Some(ref mut dev) = self.touch {
            for slot in 0u8..16 {
                if self.active_slots & (1u16 << slot) != 0 {
                    let _ = dev.emit(EV_ABS, ABS_MT_SLOT, slot as i32);
                    let _ = dev.emit(EV_ABS, ABS_MT_TRACKING_ID, -1);
                }
            }
            if self.active_slots != 0 {
                let _ = dev.emit(EV_KEY, BTN_TOUCH, 0);
                let _ = dev.emit(EV_KEY, BTN_TOOL_FINGER, 0);
                let _ = dev.syn();
            }
        }
        self.active_slots = 0;
        self.release_pad();

        if self.pen_proximity {
            if let Some(ref mut dev) = self.pen {
                let _ = dev.emit(EV_KEY, BTN_TOUCH, 0);
                let _ = dev.emit(EV_KEY, BTN_TOOL_PEN, 0);
                let _ = dev.emit(EV_KEY, BTN_TOOL_RUBBER, 0);
                let _ = dev.emit(EV_ABS, ABS_PRESSURE, 0);
                let _ = dev.syn();
            }
        }
        self.pen_proximity = false;

        if self.pen_button {
            if let Some(ref mut dev) = self.pen {
                let _ = dev.emit(EV_KEY, BTN_STYLUS, 0);
                let _ = dev.syn();
            }
        }
        self.pen_button = false;

        if self.mouse_buttons != 0 {
            if let Some(ref mut dev) = self.mouse {
                for (bit, code) in [(1u8, BTN_LEFT), (2, BTN_RIGHT), (4, BTN_MIDDLE)] {
                    if self.mouse_buttons & bit != 0 {
                        let _ = dev.emit(EV_KEY, code, 0);
                    }
                }
                let _ = dev.syn();
            }
        }
        self.mouse_buttons = 0;
        self.rel_rest = (0.0, 0.0);
        self.scroll_rest = (0, 0);
        self.release_pen_mouse();
        self.tip = Tip::None;
        self.auto_exit_at = None;
        self.end_pinch();
        // Last word: whatever any device still holds, by its own account, goes up.
        self.force_release_all(Which::All);
    }

    /// The keyboard, created on first use. `None` when uinput is not available.
    pub(in crate::input) fn keyboard(&mut self) -> Option<&mut UInputDevice> {
        if self.keyboard.is_none() {
            match UInputDevice::new_keyboard(&self.keyboard_name, self.keyboard_product) {
                Ok(dev) => self.keyboard = Some(dev),
                Err(e) => {
                    warn!("No keyboard device: {}. Key actions will do nothing.", e);
                    return None;
                }
            }
        }
        self.keyboard.as_mut()
    }

    /// Enter or leave Stylus mode. Entering it destroys the relative mouse: in Stylus mode only the
    /// pen and the absolute pointer exist, so a second pointing device can never fight the pen for
    /// the cursor.
    pub(in crate::input) fn set_stylus_mode(&mut self, on: bool) {
        self.stylus_mode = on;
        if on {
            if self.mouse.take().is_some() {
                info!("Stylus mode: relative mouse removed");
            }
            self.drop_pad();
            self.mouse_buttons = 0;
            self.rel_rest = (0.0, 0.0);
            self.scroll_rest = (0, 0);
        }
    }

    /// Create the virtual devices the current mode will need now, so the first click or key of a
    /// gesture is not lost to the compositor still discovering a device made at that moment.
    /// Touchpad mode with native gestures wants the multi-touch touchpad and a relative mouse (the
    /// right-click zone and the wake nudge); without them it wants the mouse and the keyboard for
    /// the key-combo gestures. Stylus and Display mode need none of them, and the touchpad is
    /// removed there so a second pointing device can never fight the pen or the screen.
    pub(in crate::input) fn ensure_devices_for(
        &mut self,
        mode: crate::config::InputMode,
        native: bool,
    ) {
        use crate::config::InputMode;
        match mode {
            InputMode::Touchpad => {
                self.set_stylus_mode(false);
                self.mouse();
                if native {
                    self.pad();
                } else {
                    self.drop_pad();
                    self.keyboard();
                }
            }
            InputMode::Stylus | InputMode::Display => self.drop_pad(),
        }
    }

    /// The virtual multi-touch touchpad, created on first use. `None` in Stylus mode or when uinput
    /// is not available (logged once, by the failed creation).
    pub(in crate::input) fn pad(&mut self) -> Option<&mut UInputDevice> {
        if self.stylus_mode {
            return None;
        }
        if self.pad.is_none() {
            match UInputDevice::new_touchpad(&self.pad_name, self.pad_product) {
                Ok(dev) => {
                    self.pad = Some(dev);
                    self.pad_state = PadState::new();
                }
                Err(e) => {
                    warn!(
                        "No multi-touch touchpad: {}. Native gestures will do nothing.",
                        e
                    );
                    return None;
                }
            }
        }
        self.pad.as_mut()
    }

    /// Replay the tablet's fingers on the touchpad (see [`UInputDevice::inject_pad`]).
    pub(in crate::input) fn touchpad_raw(&mut self, fingers: &[PadFinger]) {
        if self.pad().is_none() {
            return;
        }
        let mut state = std::mem::replace(&mut self.pad_state, PadState::new());
        if let Some(dev) = self.pad.as_mut() {
            if let Err(e) = dev.inject_pad(&mut state, fingers) {
                warn!("Failed to inject touchpad fingers: {}", e);
            }
        }
        self.pad_state = state;
    }

    /// Lift every finger still on the touchpad, so no swipe or pinch stays open.
    pub(in crate::input) fn release_pad(&mut self) {
        if self.pad_state.count() > 0 {
            self.touchpad_raw(&[]);
        }
    }

    /// Remove the touchpad (leaving Touchpad mode, or native gestures turned off).
    pub(in crate::input) fn drop_pad(&mut self) {
        if self.pad.is_some() {
            self.release_pad();
            self.pad = None;
            info!("Touchpad mode left: multi-touch touchpad removed");
        }
        self.pad_state = PadState::new();
    }

    /// Pen coordinates (0..=COORD_MAX over the pen's output) as pointer coordinates over the whole
    /// desktop.
    pub(in crate::input) fn desktop_xy(&self, px: i32, py: i32) -> (i32, i32) {
        let (px, py) = match self.pointer_area {
            Some((ox, oy, ow, oh)) => {
                let u = px as f64 / COORD_MAX as f64;
                let v = py as f64 / COORD_MAX as f64;
                (
                    ((ox + u * ow) * COORD_MAX as f64) as i32,
                    ((oy + v * oh) * COORD_MAX as f64) as i32,
                )
            }
            None => (px, py),
        };
        let (px, py) = (px.clamp(0, COORD_MAX), py.clamp(0, COORD_MAX));
        match self.snap_grid {
            Some((gw, gh)) if gw > 1.0 && gh > 1.0 => {
                let snap = |v: i32, n: f64| {
                    let c = COORD_MAX as f64;
                    (((v as f64 / c) * (n - 1.0)).round() / (n - 1.0) * c).round() as i32
                };
                (snap(px, gw), snap(py, gh))
            }
            _ => (px, py),
        }
    }

    /// Put the absolute pointer where the pen last was, so it can click there.
    pub(in crate::input) fn park_pointer(&mut self) {
        let (px, py) = self.desktop_xy(self.last_pen_pos.0, self.last_pen_pos.1);
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_ABS, ABS_X, px);
            let _ = dev.emit(EV_ABS, ABS_Y, py);
            let _ = dev.syn();
        }
    }

    /// Whether the pen can act as a mouse: needs the absolute pointer.
    pub(in crate::input) fn has_pointer(&self) -> bool {
        self.pointer.is_some()
    }

    /// The pen as a mouse (Stylus mode, `mouse` style). The tip is the left button: it goes down
    /// the moment the pen touches and stays down for the whole stroke, so dragging selects text
    /// and moves windows. Hovering moves the cursor. The pen's side buttons are the right and
    /// middle buttons, or nothing, as configured. The pointer is an ordinary absolute mouse, so
    /// the desktop draws its cursor.
    ///
    /// Actions are the pen protocol's: 0 down, 1 up, 2 move, 3 hover, 4 left the screen,
    /// 5/6 first side button down/up, 7/8 second side button down/up.
    pub(in crate::input) fn pen_as_mouse(
        &mut self,
        x: i32,
        y: i32,
        action: u8,
        primary: &str,
        secondary: &str,
    ) {
        let (px, py) = self.desktop_xy(x, y);
        let side = |name: &str| match name {
            "right" => Some((BTN_RIGHT, 2u8)),
            "middle" => Some((BTN_MIDDLE, 4u8)),
            _ => None,
        };
        let Some(dev) = self.pointer.as_mut() else {
            return;
        };
        let mut held = self.pen_mouse_buttons;
        let place = |dev: &mut UInputDevice| {
            let _ = dev.emit(EV_ABS, ABS_X, px);
            let _ = dev.emit(EV_ABS, ABS_Y, py);
            let _ = dev.syn();
        };
        let mut press = |dev: &mut UInputDevice, code: u16, bit: u8, down: bool| {
            let _ = dev.emit(EV_KEY, code, down as i32);
            let _ = dev.syn();
            if down {
                held |= bit;
            } else {
                held &= !bit;
            }
        };
        match action {
            0 => {
                place(dev);
                press(dev, BTN_LEFT, 1, true);
            }
            1 => {
                place(dev);
                press(dev, BTN_LEFT, 1, false);
            }
            2 | 3 => place(dev),
            5..=8 => {
                let name = if action <= 6 { primary } else { secondary };
                if let Some((code, bit)) = side(name) {
                    press(dev, code, bit, action == 5 || action == 7);
                }
            }
            _ => {}
        }
        self.pen_mouse_buttons = held;
    }

    /// The single way to let go of a device: every key, button and tool it holds goes up (read
    /// from the device's own held set, not from a parallel record that could disagree), pressure
    /// goes to zero, and the bookkeeping of that device returns to `Idle`. Call it on every
    /// transition that must not leave anything held: route change, mode change, cancel, the pen
    /// leaving range, disconnect.
    pub(in crate::input) fn force_release_all(&mut self, which: Which) {
        let all = which == Which::All;
        if all || which == Which::Pen {
            if let Some(ref mut dev) = self.pen {
                dev.force_release(true);
            }
            self.pen_proximity = false;
            self.pen_button = false;
        }
        if all || which == Which::Pointer {
            if let Some(ref mut dev) = self.pointer {
                dev.force_release(false);
            }
            self.pen_mouse_buttons = 0;
        }
        if all || which == Which::Touchpad {
            if let Some(ref mut dev) = self.mouse {
                dev.force_release(false);
            }
            self.mouse_buttons = 0;
            self.release_pad();
            if let Some(ref mut dev) = self.pad {
                dev.force_release(false);
            }
        }
        if all {
            if let Some(ref mut dev) = self.touch {
                dev.force_release(false);
            }
            self.active_slots = 0;
        }
        if all || which != Which::Touchpad {
            self.tip = Tip::None;
            self.auto_exit_at = None;
            self.auto_down_at = None;
            self.auto_last_at = None;
            self.slop_anchor = None;
            self.press_deferred = false;
            self.hold = Default::default();
            self.pending_click = None;
            self.click_release_at = None;
        }
    }

    /// The state of one device (`Idle` for a device that does not exist).
    pub(in crate::input) fn dev_state(&self, which: Which) -> DevState {
        let dev = match which {
            Which::Pen => self.pen.as_ref(),
            Which::Pointer => self.pointer.as_ref(),
            Which::Touchpad => self.mouse.as_ref(),
            Which::All => None,
        };
        dev.map(|d| d.state()).unwrap_or(DevState::Idle)
    }

    /// Let go of every button the pen holds on the absolute pointer.
    pub(in crate::input) fn release_pen_mouse(&mut self) {
        if self.pen_mouse_buttons == 0 {
            return;
        }
        let held = self.pen_mouse_buttons;
        if let Some(ref mut dev) = self.pointer {
            for (bit, code) in [(1u8, BTN_LEFT), (2, BTN_RIGHT), (4, BTN_MIDDLE)] {
                if held & bit != 0 {
                    let _ = dev.emit(EV_KEY, code, 0);
                }
            }
            let _ = dev.syn();
        }
        self.pen_mouse_buttons = 0;
    }

    /// Tap a key combination on the virtual keyboard (modifiers first, released in reverse).
    pub(in crate::input) fn tap_key_combo(&mut self, codes: &[u16]) {
        if let Some(dev) = self.keyboard() {
            if let Err(e) = dev.tap_keys(codes) {
                warn!("Failed to send keys: {}", e);
            }
        }
    }

    /// One step of a pinch: Ctrl is pressed with the first step and stays down until
    /// [`Self::end_pinch`], and the wheel turns under it. Pressing and releasing Ctrl around every
    /// step lost it to the order the compositor reads two devices in.
    pub(in crate::input) fn pinch_step(&mut self, dy: i32) {
        if dy == 0 {
            return;
        }
        if self.pinch_since.is_none() {
            let Some(kb) = self.keyboard() else { return };
            let _ = kb.emit(EV_KEY, KEY_LEFTCTRL, 1);
            let _ = kb.syn();
            // Let the compositor see Ctrl before the first notch.
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        self.pinch_since = Some(std::time::Instant::now());
        let mut rest = self.scroll_rest;
        if let Some(dev) = self.mouse() {
            let _ = dev.scroll_by(0, dy, &mut rest);
        }
        self.scroll_rest = rest;
    }

    /// The fingers came off: release Ctrl.
    pub(in crate::input) fn end_pinch(&mut self) {
        if self.pinch_since.take().is_some() {
            if let Some(kb) = self.keyboard.as_mut() {
                let _ = kb.emit(EV_KEY, KEY_LEFTCTRL, 0);
                let _ = kb.syn();
            }
        }
    }

    /// Safety net: Ctrl held for a pinch whose end never arrived is let go after this long.
    pub(in crate::input) fn expire_pinch(&mut self) {
        if self
            .pinch_since
            .is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(1500))
        {
            self.end_pinch();
        }
    }

    /// The touchpad-mode mouse, created on first use. `None` when uinput is not available, in
    /// which case the events are dropped (and logged once, by the failed creation).
    pub(in crate::input) fn mouse(&mut self) -> Option<&mut UInputDevice> {
        if self.stylus_mode {
            return None;
        }
        if self.mouse.is_none() {
            match UInputDevice::new_mouse(&self.mouse_name, self.mouse_product) {
                Ok(dev) => self.mouse = Some(dev),
                Err(e) => {
                    warn!("No touchpad device: {}. Touchpad mode will do nothing.", e);
                    return None;
                }
            }
        }
        self.mouse.as_mut()
    }
}
