//! `UInputDevice`: one virtual evdev device created through `/dev/uinput`. Constructors for each
//! device kind (touchscreen, pen, pointer, touchpad, mouse, keyboard), small helpers (key taps,
//! relative moves, scroll), and the single `emit` choke point that also tracks which keys the device
//! holds (`DevState`), so a forced release can always let go of exactly what is down.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(in crate::input) struct InputAbsInfo {
    pub(in crate::input) value: i32,
    pub(in crate::input) minimum: i32,
    pub(in crate::input) maximum: i32,
    pub(in crate::input) fuzz: i32,
    pub(in crate::input) flat: i32,
    pub(in crate::input) resolution: i32,
}

#[repr(C)]
pub(in crate::input) struct UinputAbsSetup {
    pub(in crate::input) code: u16,
    pub(in crate::input) _pad: u16,
    pub(in crate::input) absinfo: InputAbsInfo,
}

#[repr(C)]
pub(in crate::input) struct UinputSetup {
    pub(in crate::input) bustype: u16,
    pub(in crate::input) vendor: u16,
    pub(in crate::input) product: u16,
    pub(in crate::input) version: u16,
    pub(in crate::input) name: [u8; 80],
    pub(in crate::input) ff_effects_max: u32,
}

// Linux input_event struct (for writing to uinput)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(in crate::input) struct LinuxInputEvent {
    pub(in crate::input) tv_sec: i64,
    pub(in crate::input) tv_usec: i64,
    pub(in crate::input) type_: u16,
    pub(in crate::input) code: u16,
    pub(in crate::input) value: i32,
}

/// A uinput virtual input device for injecting touch/pen events into Linux.
/// Touch and pen are SEPARATE devices: libinput classifies a touchscreen and
/// a tablet pen differently and rejects a device that mixes both.
pub(in crate::input) struct UInputDevice {
    pub(in crate::input) file: File,
    /// Every key/button this device has been told is down, as bits (see [`key_bit`]). Updated by
    /// [`UInputDevice::emit`] alone, so it is true whatever code path did the emitting: the one
    /// source for [`DevState`] and for [`UInputDevice::force_release`].
    pub(in crate::input) held: u32,
}

/// The bit of `held` a key code owns: buttons in the low five bits, tools above them.
pub(in crate::input) fn key_bit(code: u16) -> Option<u32> {
    Some(match code {
        BTN_LEFT => 1 << 0,
        BTN_RIGHT => 1 << 1,
        BTN_MIDDLE => 1 << 2,
        BTN_TOUCH => 1 << 3,
        BTN_STYLUS => 1 << 4,
        BTN_TOOL_PEN => 1 << 5,
        BTN_TOOL_RUBBER => 1 << 6,
        BTN_TOOL_FINGER => 1 << 7,
        BTN_TOOL_DOUBLETAP => 1 << 8,
        BTN_TOOL_TRIPLETAP => 1 << 9,
        BTN_TOOL_QUADTAP => 1 << 10,
        BTN_TOOL_QUINTTAP => 1 << 11,
        _ => return None,
    })
}

/// The codes `held` bits stand for, in bit order.
const HELD_CODES: [u16; 12] = [
    BTN_LEFT,
    BTN_RIGHT,
    BTN_MIDDLE,
    BTN_TOUCH,
    BTN_STYLUS,
    BTN_TOOL_PEN,
    BTN_TOOL_RUBBER,
    BTN_TOOL_FINGER,
    BTN_TOOL_DOUBLETAP,
    BTN_TOOL_TRIPLETAP,
    BTN_TOOL_QUADTAP,
    BTN_TOOL_QUINTTAP,
];

/// Mask of the button bits (left, right, middle, tip contact, barrel) in `held`.
const BUTTON_BITS: u32 = 0b1_1111;

/// What a virtual device is doing, derived from what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::input) enum DevState {
    /// Nothing held: no button, no tool in range.
    Idle,
    /// A tool is in range (pen near the glass, fingers on the pad) but no button is down.
    Hovering,
    /// At least one button or the tip contact is down; the mask has bit 0 left, 1 right, 2 middle,
    /// 3 tip contact, 4 barrel button.
    Down(u8),
}

impl UInputDevice {
    pub(in crate::input) fn open_uinput() -> Result<File> {
        OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .context("Failed to open /dev/uinput. Ensure the uinput module is loaded and you have permissions (try: sudo modprobe uinput)")
    }

    /// Touchscreen device: multitouch + single-touch axes, INPUT_PROP_DIRECT.
    pub(in crate::input) fn new_touch(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();
        let w = COORD_MAX + 1;
        let h = COORD_MAX + 1;

        unsafe {
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_ABS as i32)?;
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT)?;

            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_TOUCH as i32)?;
            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_TOOL_FINGER as i32)?;

            Self::abs_setup(fd, ABS_X, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_Y, 0, h - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_PRESSURE, 0, 4096, 0)?;
            Self::abs_setup(fd, ABS_MT_SLOT, 0, 9, 0)?;
            Self::abs_setup(fd, ABS_MT_POSITION_X, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_POSITION_Y, 0, h - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_TRACKING_ID, 0, 65535, 0)?;
            Self::abs_setup(fd, ABS_MT_PRESSURE, 0, 4096, 0)?;

            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput touchscreen '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file, held: 0 })
    }

    /// Pen tablet device: stylus tool + pressure + tilt.
    /// No INPUT_PROP_DIRECT — that flag means "touchscreen" and causes KDE to
    /// activate the on-screen keyboard on every pen tap. Without it, libinput
    /// classifies this as a tablet tool (Wacom-style): the cursor follows the
    /// pen position and clicks work as mouse clicks.
    pub(in crate::input) fn new_pen(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();
        let w = COORD_MAX + 1;
        let h = COORD_MAX + 1;

        unsafe {
            // A pointing tablet, not a screen: INPUT_PROP_POINTER, never INPUT_PROP_DIRECT.
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_POINTER)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_ABS as i32)?;

            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_TOUCH as i32)?;
            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_TOOL_PEN as i32)?;
            // Eraser end: reported as TOOL_TYPE_ERASER on Android, mapped to
            // BTN_TOOL_RUBBER here so GIMP's eraser tool follows the pen.
            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_TOOL_RUBBER as i32)?;
            // libinput requires the stylus button capability on pen devices
            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_STYLUS as i32)?;

            Self::abs_setup(fd, ABS_X, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_Y, 0, h - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_PRESSURE, 0, PEN_PRESSURE_MAX, 0)?;
            // Tilt in whole degrees
            Self::abs_setup(fd, ABS_TILT_X, -90, 90, 0)?;
            Self::abs_setup(fd, ABS_TILT_Y, -90, 90, 0)?;

            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput pen tablet '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file, held: 0 })
    }

    /// An absolute-positioning pointer, the same shape as a VM's virtual
    /// tablet. It exists for one reason: a tablet tool's cursor is hidden the
    /// moment the tool leaves proximity, which is correct for a Wacom on a desk
    /// but wrong here — lift the pen and you lose all sense of where you were
    /// pointing. Parking this pointer at the last pen position leaves an
    /// ordinary mouse cursor sitting there.
    ///
    /// No INPUT_PROP_DIRECT (that would make it a touchscreen and bring the
    /// on-screen keyboard with it) and no BTN_TOOL_PEN (that would make it a
    /// second tablet).
    pub(in crate::input) fn new_pointer(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();
        let w = COORD_MAX + 1;

        unsafe {
            // An absolute mouse: INPUT_PROP_POINTER, never INPUT_PROP_DIRECT or a touch class.
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_POINTER)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_ABS as i32)?;
            // Right / middle button and the wheel exist so the pen's buttons can click in Stylus mode
            // without a relative mouse (which Stylus mode must not have).
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_REL as i32)?;
            for b in [BTN_LEFT, BTN_RIGHT, BTN_MIDDLE] {
                Self::ioctl_val(fd, UI_SET_KEYBIT, b as i32)?;
            }
            for r in [REL_WHEEL, REL_HWHEEL, REL_WHEEL_HI_RES, REL_HWHEEL_HI_RES] {
                Self::ioctl_val(fd, UI_SET_RELBIT, r as i32)?;
            }
            Self::abs_setup(fd, ABS_X, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_Y, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput pointer '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file, held: 0 })
    }

    /// A multi-touch touchpad, like the clickpad of a laptop. The tablet's raw fingers are replayed
    /// on it and libinput does the rest: pointer motion, taps, two-finger scroll, pinch, and 3 / 4
    /// finger swipes, which GNOME and KDE turn into workspace and overview gestures.
    ///
    /// Declared the way a real clickpad is: MT protocol B slots with tracking ids, finger-count tool
    /// buttons (`BTN_TOOL_FINGER` .. `BTN_TOOL_QUINTTAP`), a pressure axis, a real resolution (libinput
    /// measures every threshold in millimetres), INPUT_PROP_POINTER and INPUT_PROP_BUTTONPAD. Not
    /// INPUT_PROP_DIRECT: that is a touchscreen and would put the finger on the screen.
    pub(in crate::input) fn new_touchpad(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();
        let w = (PAD_WIDTH_MM * PAD_UNITS_PER_MM as f64) as i32;
        let h = (PAD_HEIGHT_MM * PAD_UNITS_PER_MM as f64) as i32;

        unsafe {
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_POINTER)?;
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_BUTTONPAD)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_ABS as i32)?;

            for key in [
                BTN_LEFT,
                BTN_TOUCH,
                BTN_TOOL_FINGER,
                BTN_TOOL_DOUBLETAP,
                BTN_TOOL_TRIPLETAP,
                BTN_TOOL_QUADTAP,
                BTN_TOOL_QUINTTAP,
            ] {
                Self::ioctl_val(fd, UI_SET_KEYBIT, key as i32)?;
            }

            Self::abs_setup(fd, ABS_X, 0, w - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_Y, 0, h - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_PRESSURE, 0, PAD_PRESSURE_MAX, 0)?;
            Self::abs_setup(fd, ABS_MT_SLOT, 0, PAD_SLOTS as i32 - 1, 0)?;
            Self::abs_setup(fd, ABS_MT_POSITION_X, 0, w - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_POSITION_Y, 0, h - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_TRACKING_ID, 0, 65535, 0)?;
            Self::abs_setup(fd, ABS_MT_PRESSURE, 0, PAD_PRESSURE_MAX, 0)?;

            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput touchpad '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file, held: 0 })
    }

    /// A plain relative mouse. Created the first time touchpad mode sends something, so people
    /// who never use it never get the extra device.
    ///
    /// It must look like a standard mouse and nothing else: INPUT_PROP_POINTER set, and none of
    /// the touchscreen / touchpad properties (INPUT_PROP_DIRECT in particular), with only
    /// EV_REL motion and mouse buttons. A device that reads as a touch device makes Mutter hide
    /// the cursor.
    pub(in crate::input) fn new_mouse(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();

        unsafe {
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_POINTER)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_REL as i32)?;
            for b in [BTN_LEFT, BTN_RIGHT, BTN_MIDDLE] {
                Self::ioctl_val(fd, UI_SET_KEYBIT, b as i32)?;
            }
            for r in [
                REL_X,
                REL_Y,
                REL_WHEEL,
                REL_HWHEEL,
                REL_WHEEL_HI_RES,
                REL_HWHEEL_HI_RES,
            ] {
                Self::ioctl_val(fd, UI_SET_RELBIT, r as i32)?;
            }
            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput mouse '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file, held: 0 })
    }

    /// A plain keyboard, made the first time a touchpad gesture needs a key.
    /// It declares the ordinary keys (1..=248 covers letters, digits, function keys, arrows and
    /// the modifiers) and nothing else, so it reads as a standard keyboard.
    pub(in crate::input) fn new_keyboard(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();

        unsafe {
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            for key in 1..=248 {
                Self::ioctl_val(fd, UI_SET_KEYBIT, key)?;
            }
            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput keyboard '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file, held: 0 })
    }

    /// Tap a key combination: press every code in order, then release them in reverse.
    pub(in crate::input) fn tap_keys(&mut self, codes: &[u16]) -> Result<()> {
        if codes.is_empty() {
            return Ok(());
        }
        for &c in codes {
            self.emit(EV_KEY, c, 1)?;
        }
        self.syn()?;
        for &c in codes.iter().rev() {
            self.emit(EV_KEY, c, 0)?;
        }
        self.syn()
    }

    /// Move the pointer by whole pixels; a zero move writes nothing.
    pub(in crate::input) fn move_by(&mut self, dx: i32, dy: i32) -> Result<()> {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        if dx != 0 {
            self.emit(EV_REL, REL_X, dx)?;
        }
        if dy != 0 {
            self.emit(EV_REL, REL_Y, dy)?;
        }
        self.syn()
    }

    /// Press or release a mouse button (0 left, 1 right, 2 middle).
    pub(in crate::input) fn press(&mut self, button: u8, down: bool) -> Result<()> {
        let code = match button {
            1 => BTN_RIGHT,
            2 => BTN_MIDDLE,
            _ => BTN_LEFT,
        };
        self.emit(EV_KEY, code, down as i32)?;
        self.syn()
    }

    /// Scroll by `dx`/`dy` in high-resolution units (120 per wheel notch, the kernel's
    /// convention). The legacy whole-notch axes are fed from the remainder so applications that
    /// ignore the hi-res ones still scroll.
    pub(in crate::input) fn scroll_by(&mut self, dx: i32, dy: i32, rest: &mut (i32, i32)) -> Result<()> {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        if dy != 0 {
            self.emit(EV_REL, REL_WHEEL_HI_RES, dy)?;
            rest.1 += dy;
            let notches = rest.1 / 120;
            if notches != 0 {
                self.emit(EV_REL, REL_WHEEL, notches)?;
                rest.1 -= notches * 120;
            }
        }
        if dx != 0 {
            self.emit(EV_REL, REL_HWHEEL_HI_RES, dx)?;
            rest.0 += dx;
            let notches = rest.0 / 120;
            if notches != 0 {
                self.emit(EV_REL, REL_HWHEEL, notches)?;
                rest.0 -= notches * 120;
            }
        }
        self.syn()
    }

    unsafe fn abs_setup(fd: i32, code: u16, min: i32, max: i32, resolution: i32) -> Result<()> {
        let setup = UinputAbsSetup {
            code,
            _pad: 0,
            absinfo: InputAbsInfo {
                value: 0,
                minimum: min,
                maximum: max,
                fuzz: 0,
                flat: 0,
                resolution,
            },
        };
        Self::ioctl_val(fd, UI_SET_ABSBIT, code as i32)?;
        if libc::ioctl(fd, UI_ABS_SETUP, &setup as *const UinputAbsSetup) < 0 {
            anyhow::bail!(
                "UI_ABS_SETUP({:#x}) failed: {}",
                code,
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }

    unsafe fn dev_setup_and_create(fd: i32, name: &str, product: u16) -> Result<()> {
        let mut setup = UinputSetup {
            bustype: BUS_VIRTUAL,
            vendor: UINPUT_VENDOR,
            product,
            version: 1,
            name: [0u8; 80],
            ff_effects_max: 0,
        };
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(79);
        setup.name[..len].copy_from_slice(&name_bytes[..len]);

        if libc::ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup) < 0 {
            anyhow::bail!("UI_DEV_SETUP failed: {}", std::io::Error::last_os_error());
        }
        if libc::ioctl(fd, UI_DEV_CREATE) < 0 {
            anyhow::bail!("UI_DEV_CREATE failed: {}", std::io::Error::last_os_error());
        }
        Ok(())
    }

    unsafe fn ioctl_val(fd: i32, request: libc::c_ulong, value: i32) -> Result<()> {
        if libc::ioctl(fd, request as libc::c_ulong, value) < 0 {
            anyhow::bail!(
                "ioctl({:#x}, {}) failed: {}",
                request,
                value,
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }

    pub(in crate::input) fn emit(&mut self, type_: u16, code: u16, value: i32) -> Result<()> {
        if type_ == EV_KEY {
            if let Some(bit) = key_bit(code) {
                match value {
                    0 => self.held &= !bit,
                    1 => self.held |= bit,
                    _ => {}
                }
            }
        }
        self.write_raw(type_, code, value)
    }

    /// The device's state, derived from what it holds (see [`DevState`]).
    pub(in crate::input) fn state(&self) -> DevState {
        let buttons = self.held & BUTTON_BITS;
        if buttons != 0 {
            DevState::Down(buttons as u8)
        } else if self.held != 0 {
            DevState::Hovering
        } else {
            DevState::Idle
        }
    }

    /// Release every key and tool the device holds, and zero the pressure axis if it has one.
    /// Returns whether anything was held. Costs nothing on an idle device.
    pub(in crate::input) fn force_release(&mut self, pressure_axis: bool) -> bool {
        if self.held == 0 {
            return false;
        }
        for (i, code) in HELD_CODES.iter().enumerate() {
            if self.held & (1 << i) != 0 {
                let _ = self.emit(EV_KEY, *code, 0);
            }
        }
        if pressure_axis {
            let _ = self.emit(EV_ABS, ABS_PRESSURE, 0);
        }
        let _ = self.syn();
        self.held = 0;
        true
    }

    fn write_raw(&mut self, type_: u16, code: u16, value: i32) -> Result<()> {
        let ev = LinuxInputEvent {
            tv_sec: 0,
            tv_usec: 0,
            type_,
            code,
            value,
        };
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &ev as *const LinuxInputEvent as *const u8,
                std::mem::size_of::<LinuxInputEvent>(),
            )
        };
        self.file.write_all(bytes)?;
        Ok(())
    }

    pub(in crate::input) fn syn(&mut self) -> Result<()> {
        self.emit(EV_SYN, SYN_REPORT, 0)?;
        self.file.flush()?;
        Ok(())
    }

}
impl Drop for UInputDevice {
    fn drop(&mut self) {
        unsafe {
            let fd = self.file.as_raw_fd();
            libc::ioctl(fd, UI_DEV_DESTROY as libc::c_ulong);
        }
        info!("uinput device destroyed");
    }
}
