//! Virtual input devices: Linux input / uinput constants, [`DeviceIdentity`] (names and product ids
//! per tablet), and the `UInputDevice` / `InjectDevices` wrappers that create the devices through
//! `/dev/uinput` and write evdev events into them.
//!
//! uinput is used instead of compositor APIs so touch, pen and pointer input works the same on X11,
//! GNOME, KDE and wlroots.

use super::*;

// Linux input event constants
pub(super) const EV_SYN: u16 = 0x00;
pub(super) const EV_KEY: u16 = 0x01;
pub(super) const EV_REL: u16 = 0x02;
pub(super) const EV_ABS: u16 = 0x03;

pub(super) const SYN_REPORT: u16 = 0x00;

pub(super) const BTN_TOUCH: u16 = 0x14a;
pub(super) const BTN_TOOL_FINGER: u16 = 0x145;
pub(super) const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
pub(super) const BTN_TOOL_TRIPLETAP: u16 = 0x14e;
pub(super) const BTN_TOOL_PEN: u16 = 0x140;
pub(super) const BTN_TOOL_RUBBER: u16 = 0x141;

pub(super) const ABS_X: u16 = 0x00;
pub(super) const ABS_Y: u16 = 0x01;
pub(super) const ABS_PRESSURE: u16 = 0x18;
pub(super) const ABS_MT_SLOT: u16 = 0x2f;
pub(super) const ABS_MT_POSITION_X: u16 = 0x35;
pub(super) const ABS_MT_POSITION_Y: u16 = 0x36;
pub(super) const ABS_MT_TRACKING_ID: u16 = 0x39;
pub(super) const ABS_MT_PRESSURE: u16 = 0x3a;
pub(super) const ABS_TILT_X: u16 = 0x1a;
pub(super) const ABS_TILT_Y: u16 = 0x1b;

pub(super) const BTN_STYLUS: u16 = 0x14b;
pub(super) const BTN_LEFT: u16 = 0x110;
pub(super) const BTN_RIGHT: u16 = 0x111;
pub(super) const BTN_MIDDLE: u16 = 0x112;

pub(super) const REL_X: u16 = 0x00;
pub(super) const REL_Y: u16 = 0x01;
pub(super) const REL_HWHEEL: u16 = 0x06;
pub(super) const REL_WHEEL: u16 = 0x08;
pub(super) const REL_WHEEL_HI_RES: u16 = 0x0b;
pub(super) const REL_HWHEEL_HI_RES: u16 = 0x0c;

// uinput ioctl constants (modern UI_DEV_SETUP/UI_ABS_SETUP API — the legacy
// uinput_user_dev write() API cannot declare axis resolution, which makes
// libinput reject the device: "missing tablet capabilities ... resolution")
pub(super) const UI_SET_EVBIT: libc::c_ulong = 0x40045564;
pub(super) const UI_SET_KEYBIT: libc::c_ulong = 0x40045565;
pub(super) const UI_SET_RELBIT: libc::c_ulong = 0x40045566;
pub(super) const UI_SET_ABSBIT: libc::c_ulong = 0x40045567;
pub(super) const UI_SET_PROPBIT: libc::c_ulong = 0x4004556e;
pub(super) const UI_DEV_SETUP: libc::c_ulong = 0x405c5503;
pub(super) const UI_ABS_SETUP: libc::c_ulong = 0x401c5504;
pub(super) const UI_DEV_CREATE: libc::c_ulong = 0x5501;
pub(super) const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

pub(super) const BUS_VIRTUAL: u16 = 0x06;
pub(super) const INPUT_PROP_DIRECT: i32 = 0x01;
pub(super) const INPUT_PROP_BUTTONPAD: i32 = 0x02;
/// "This is a pointing device that needs a cursor" (value 0). Declared on the touchpad-mode mouse
/// so the compositor treats it as an ordinary mouse. Deliberately not set on it: INPUT_PROP_DIRECT
/// (touchscreen), INPUT_PROP_BUTTONPAD and INPUT_PROP_SEMI_MT (touchpad classes).
pub(super) const INPUT_PROP_POINTER: i32 = 0x00;

/// Coordinates are injected in a fixed 0..65535 space — the compositor maps
/// the device onto the output, so the virtual display resolution can change
/// at runtime without recreating uinput devices.
pub(super) const COORD_MAX: i32 = 65535;

/// Identity of the virtual input devices. These must stay stable: KDE keys the
/// device→output association in kcminputrc on vendor/product/name, so changing
/// any of them silently orphans the mapping written by `map_devices_to_output`.
pub(super) const UINPUT_VENDOR: u16 = 0x4553;
pub(super) const PRODUCT_TOUCH: u16 = 0x0001;
pub(super) const PRODUCT_PEN: u16 = 0x0002;
pub(super) const PRODUCT_POINTER: u16 = 0x0003;
pub(super) const PRODUCT_TOUCHPAD: u16 = 0x0004;
pub(super) const PRODUCT_KEYBOARD: u16 = 0x0005;
pub(super) const PRODUCT_GESTURES: u16 = 0x0006;
pub(super) const TOUCH_DEVICE_NAME: &str = "UScreen Touch";
pub(super) const PEN_DEVICE_NAME: &str = "UScreen Pen";
pub(super) const POINTER_DEVICE_NAME: &str = "UScreen Pointer";
pub(super) const TOUCHPAD_DEVICE_NAME: &str = "UScreen Touchpad";
pub(super) const KEYBOARD_DEVICE_NAME: &str = "UScreen Keyboard";
pub(super) const GESTURES_DEVICE_NAME: &str = "UScreen Gestures";

/// The virtual gesture pad is 100 x 60 mm (20 units per mm): a laptop-sized touchpad, so libinput's
/// millimetre thresholds for a pinch behave as they do on real hardware.
pub(super) const PAD_W: i32 = 2000;
pub(super) const PAD_H: i32 = 1200;
pub(super) const PAD_UNITS_PER_MM: i32 = 20;
/// Distance between the two virtual fingers when a pinch begins (30 mm), and the range they are
/// kept inside. A pinch that would leave the range ends and begins again from the start distance.
const PINCH_START: f64 = 600.0;
const PINCH_MIN: f64 = 240.0;
const PINCH_MAX: f64 = 1100.0;
/// Wheel units per natural-log unit of zoom for the Ctrl + wheel fallback (used only when the
/// gesture pad cannot be created): zooming to 1.5x is about 2.4 notches.
const FALLBACK_UNITS_PER_LN: f64 = 700.0;

/// Device names and product ids for the N-th tablet. The first keeps the
/// original names and ids so existing KWin input mappings stay valid; every
/// further one gets a numbered name and its own product range, since KWin
/// keys its per-device settings on vendor/product/name.
#[derive(Clone, Debug)]
pub struct DeviceIdentity {
    pub touch: String,
    pub pen: String,
    pub pointer: String,
    /// Plain relative mouse used by touchpad mode. Created on first use and never mapped to an
    /// output (a relative device has no screen of its own).
    pub touchpad: String,
    /// Virtual keyboard for stylus-profile key combinations and touchpad gestures. Created on
    /// first use.
    pub keyboard: String,
    /// Virtual multi-touch touchpad that carries pinch / rotate gestures. Created on first use.
    pub gestures: String,
    pub product_touch: u16,
    pub product_pen: u16,
    pub product_pointer: u16,
    pub product_touchpad: u16,
    pub product_keyboard: u16,
    pub product_gestures: u16,
    /// How many of the three devices exist: 2 when pointer_handoff is off.
    pub device_count: usize,
}

impl DeviceIdentity {
    /// Identity for the n-th tablet: names get a numeric suffix and product ids are offset by 16 per instance so the compositor sees distinct devices.
    pub fn for_instance(instance: u32) -> Self {
        let suffix = if instance == 0 {
            String::new()
        } else {
            format!(" {}", instance + 1)
        };
        let base = (instance as u16) * 16;
        Self {
            touch: format!("{}{}", TOUCH_DEVICE_NAME, suffix),
            pen: format!("{}{}", PEN_DEVICE_NAME, suffix),
            pointer: format!("{}{}", POINTER_DEVICE_NAME, suffix),
            touchpad: format!("{}{}", TOUCHPAD_DEVICE_NAME, suffix),
            keyboard: format!("{}{}", KEYBOARD_DEVICE_NAME, suffix),
            gestures: format!("{}{}", GESTURES_DEVICE_NAME, suffix),
            product_touch: PRODUCT_TOUCH + base,
            product_pen: PRODUCT_PEN + base,
            product_pointer: PRODUCT_POINTER + base,
            product_touchpad: PRODUCT_TOUCHPAD + base,
            product_keyboard: PRODUCT_KEYBOARD + base,
            product_gestures: PRODUCT_GESTURES + base,
            device_count: 3,
        }
    }
    pub(super) fn owns(&self, name: &str) -> bool {
        name == self.touch || name == self.pen || name == self.pointer
    }
}
/// ~310mm wide active area → 65535/310 ≈ 211 units/mm.
/// libinput requires a resolution on touchscreen/tablet axes.
pub(super) const RESOLUTION_UNITS_PER_MM: i32 = 211;
/// The virtual pen's pressure axis runs 0..=PEN_PRESSURE_MAX.
pub(super) const PEN_PRESSURE_MAX: i32 = 4096;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct InputAbsInfo {
    pub(super) value: i32,
    pub(super) minimum: i32,
    pub(super) maximum: i32,
    pub(super) fuzz: i32,
    pub(super) flat: i32,
    pub(super) resolution: i32,
}

#[repr(C)]
pub(super) struct UinputAbsSetup {
    pub(super) code: u16,
    pub(super) _pad: u16,
    pub(super) absinfo: InputAbsInfo,
}

#[repr(C)]
pub(super) struct UinputSetup {
    pub(super) bustype: u16,
    pub(super) vendor: u16,
    pub(super) product: u16,
    pub(super) version: u16,
    pub(super) name: [u8; 80],
    pub(super) ff_effects_max: u32,
}

// Linux input_event struct (for writing to uinput)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(super) struct LinuxInputEvent {
    pub(super) tv_sec: i64,
    pub(super) tv_usec: i64,
    pub(super) type_: u16,
    pub(super) code: u16,
    pub(super) value: i32,
}

/// A uinput virtual input device for injecting touch/pen events into Linux.
/// Touch and pen are SEPARATE devices: libinput classifies a touchscreen and
/// a tablet pen differently and rejects a device that mixes both.
pub(super) struct UInputDevice {
    pub(super) file: File,
}

impl UInputDevice {
    pub(super) fn open_uinput() -> Result<File> {
        OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .context("Failed to open /dev/uinput. Ensure the uinput module is loaded and you have permissions (try: sudo modprobe uinput)")
    }

    /// Touchscreen device: multitouch + single-touch axes, INPUT_PROP_DIRECT.
    pub(super) fn new_touch(name: &str, product: u16) -> Result<Self> {
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
        Ok(Self { file })
    }

    /// Pen tablet device: stylus tool + pressure + tilt.
    /// No INPUT_PROP_DIRECT — that flag means "touchscreen" and causes KDE to
    /// activate the on-screen keyboard on every pen tap. Without it, libinput
    /// classifies this as a tablet tool (Wacom-style): the cursor follows the
    /// pen position and clicks work as mouse clicks.
    pub(super) fn new_pen(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();
        let w = COORD_MAX + 1;
        let h = COORD_MAX + 1;

        unsafe {
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
        Ok(Self { file })
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
    pub(super) fn new_pointer(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();
        let w = COORD_MAX + 1;

        unsafe {
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_ABS as i32)?;
            // Right / middle button and the wheel exist so a stylus profile can click and scroll in
            // Stylus mode without a relative mouse (which Stylus mode must not have).
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
        Ok(Self { file })
    }

    /// A plain relative mouse. Created the first time touchpad mode sends something, so people
    /// who never use it never get the extra device.
    ///
    /// It must look like a standard mouse and nothing else: INPUT_PROP_POINTER set, and none of
    /// the touchscreen / touchpad properties (INPUT_PROP_DIRECT in particular), with only
    /// EV_REL motion and mouse buttons. A device that reads as a touch device makes Mutter hide
    /// the cursor.
    pub(super) fn new_mouse(name: &str, product: u16) -> Result<Self> {
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
        Ok(Self { file })
    }

    /// A plain keyboard, made the first time a stylus profile or touchpad gesture needs a key.
    /// It declares the ordinary keys (1..=248 covers letters, digits, function keys, arrows and
    /// the modifiers) and nothing else, so it reads as a standard keyboard.
    pub(super) fn new_keyboard(name: &str, product: u16) -> Result<Self> {
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
        Ok(Self { file })
    }

    /// A multi-touch touchpad that is only ever touched by two synthetic fingers during a pinch or
    /// rotate. It is a real touchpad as far as udev, libinput and the compositor can tell
    /// (INPUT_PROP_POINTER + BUTTONPAD, `BTN_TOOL_*TAP`, MT slots), so they recognise the two
    /// fingers' motion as a pinch gesture and hand it to the application natively.
    pub(super) fn new_gesture_pad(name: &str, product: u16) -> Result<Self> {
        let file = Self::open_uinput()?;
        let fd = file.as_raw_fd();

        unsafe {
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_POINTER)?;
            Self::ioctl_val(fd, UI_SET_PROPBIT, INPUT_PROP_BUTTONPAD)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            Self::ioctl_val(fd, UI_SET_EVBIT, EV_ABS as i32)?;
            for k in [
                BTN_LEFT,
                BTN_TOUCH,
                BTN_TOOL_FINGER,
                BTN_TOOL_DOUBLETAP,
                BTN_TOOL_TRIPLETAP,
            ] {
                Self::ioctl_val(fd, UI_SET_KEYBIT, k as i32)?;
            }
            Self::abs_setup(fd, ABS_X, 0, PAD_W - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_Y, 0, PAD_H - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_SLOT, 0, 4, 0)?;
            Self::abs_setup(fd, ABS_MT_POSITION_X, 0, PAD_W - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_POSITION_Y, 0, PAD_H - 1, PAD_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_MT_TRACKING_ID, 0, 65535, 0)?;
            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput gesture pad '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file })
    }

    /// Both virtual fingers down, `PINCH_START` apart on the horizontal.
    pub(super) fn pad_begin(&mut self, tracking_base: i32) -> Result<()> {
        let (x0, x1, y) = pinch_points(PINCH_START, 0.0);
        for (slot, x) in [(0, x0), (1, x1)] {
            self.emit(EV_ABS, ABS_MT_SLOT, slot)?;
            self.emit(EV_ABS, ABS_MT_TRACKING_ID, tracking_base + slot)?;
            self.emit(EV_ABS, ABS_MT_POSITION_X, x)?;
            self.emit(EV_ABS, ABS_MT_POSITION_Y, y.0)?;
        }
        self.emit(EV_KEY, BTN_TOUCH, 1)?;
        self.emit(EV_KEY, BTN_TOOL_DOUBLETAP, 1)?;
        self.emit(EV_ABS, ABS_X, x0)?;
        self.emit(EV_ABS, ABS_Y, y.0)?;
        self.syn()
    }

    /// Move the two fingers to `dist` apart at `angle` radians (clockwise on screen) around the pad centre.
    pub(super) fn pad_move(&mut self, dist: f64, angle: f64) -> Result<()> {
        let (x0, x1, (y0, y1)) = pinch_points(dist, angle);
        for (slot, x, y) in [(0, x0, y0), (1, x1, y1)] {
            self.emit(EV_ABS, ABS_MT_SLOT, slot)?;
            self.emit(EV_ABS, ABS_MT_POSITION_X, x)?;
            self.emit(EV_ABS, ABS_MT_POSITION_Y, y)?;
        }
        self.emit(EV_ABS, ABS_X, x0)?;
        self.emit(EV_ABS, ABS_Y, y0)?;
        self.syn()
    }

    /// Both virtual fingers up.
    pub(super) fn pad_end(&mut self) -> Result<()> {
        for slot in 0..2 {
            self.emit(EV_ABS, ABS_MT_SLOT, slot)?;
            self.emit(EV_ABS, ABS_MT_TRACKING_ID, -1)?;
        }
        self.emit(EV_KEY, BTN_TOUCH, 0)?;
        self.emit(EV_KEY, BTN_TOOL_DOUBLETAP, 0)?;
        self.syn()
    }

    /// Tap a key combination: press every code in order, then release them in reverse.
    pub(super) fn tap_keys(&mut self, codes: &[u16]) -> Result<()> {
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
    pub(super) fn move_by(&mut self, dx: i32, dy: i32) -> Result<()> {
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
    pub(super) fn press(&mut self, button: u8, down: bool) -> Result<()> {
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
    pub(super) fn scroll_by(&mut self, dx: i32, dy: i32, rest: &mut (i32, i32)) -> Result<()> {
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

    pub(super) fn emit(&mut self, type_: u16, code: u16, value: i32) -> Result<()> {
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

    pub(super) fn syn(&mut self) -> Result<()> {
        self.emit(EV_SYN, SYN_REPORT, 0)?;
        self.file.flush()?;
        Ok(())
    }

    pub(super) fn inject_touch(
        &mut self,
        x: i32,
        y: i32,
        pressure: i32,
        action: u8,
        slot: u8,
    ) -> Result<()> {
        match action {
            0 => {
                // DOWN
                self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
                self.emit(EV_ABS, ABS_MT_TRACKING_ID, slot as i32)?;
                self.emit(EV_ABS, ABS_MT_POSITION_X, x)?;
                self.emit(EV_ABS, ABS_MT_POSITION_Y, y)?;
                self.emit(EV_ABS, ABS_MT_PRESSURE, pressure)?;
                self.emit(EV_KEY, BTN_TOUCH, 1)?;
                self.emit(EV_KEY, BTN_TOOL_FINGER, 1)?;
                self.emit(EV_ABS, ABS_X, x)?;
                self.emit(EV_ABS, ABS_Y, y)?;
                self.emit(EV_ABS, ABS_PRESSURE, pressure)?;
                self.syn()?;
            }
            1 => {
                // UP
                self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
                self.emit(EV_ABS, ABS_MT_TRACKING_ID, -1)?;
                self.emit(EV_KEY, BTN_TOUCH, 0)?;
                self.emit(EV_KEY, BTN_TOOL_FINGER, 0)?;
                self.emit(EV_ABS, ABS_PRESSURE, 0)?;
                self.syn()?;
            }
            2 => {
                // MOVE - combine with previous if possible
                self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
                self.emit(EV_ABS, ABS_MT_POSITION_X, x)?;
                self.emit(EV_ABS, ABS_MT_POSITION_Y, y)?;
                self.emit(EV_ABS, ABS_MT_PRESSURE, pressure)?;
                self.emit(EV_ABS, ABS_X, x)?;
                self.emit(EV_ABS, ABS_Y, y)?;
                self.emit(EV_ABS, ABS_PRESSURE, pressure)?;
                self.syn()?;
            }
            _ => {}
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn inject_pen(
        &mut self,
        x: i32,
        y: i32,
        pressure: i32,
        tilt_x: i32,
        tilt_y: i32,
        action: u8,
        eraser: bool,
    ) -> Result<()> {
        // The active tablet-tool key depends on which end of the pen is in
        // use. An S Pen flipped to its eraser end reports TOOL_TYPE_ERASER.
        let tool = if eraser {
            BTN_TOOL_RUBBER
        } else {
            BTN_TOOL_PEN
        };
        // A pen that is touching reports some pressure. A very light tap can
        // arrive from the tablet as exactly 0 (a Xiaomi Focus Pen Pro does),
        // which says "touching" with BTN_TOUCH and "not touching" with the
        // pressure, and libinput, which decides tip-down from pressure when
        // the device has a pressure axis, then drops the tap. Hover and the
        // lift keep sending 0.
        let contact_pressure = pressure.clamp(1, PEN_PRESSURE_MAX);
        match action {
            0 => {
                // DOWN, in two frames.
                //
                // A tablet tool has to enter proximity before it can touch:
                // libinput wants to see the tool appear at a position, and only
                // then the tip come down. Announcing both in a single event
                // frame makes the first tap after picking up the pen land at
                // the previous cursor position, or get dropped entirely.
                self.emit(EV_KEY, tool, 1)?;
                self.emit(EV_ABS, ABS_X, x)?;
                self.emit(EV_ABS, ABS_Y, y)?;
                self.emit(EV_ABS, ABS_TILT_X, tilt_x)?;
                self.emit(EV_ABS, ABS_TILT_Y, tilt_y)?;
                self.syn()?;

                self.emit(EV_KEY, BTN_TOUCH, 1)?;
                self.emit(EV_ABS, ABS_PRESSURE, contact_pressure)?;
                self.syn()?;
            }
            1 => {
                // UP, in two frames, mirroring DOWN.
                //
                // The lift position is the pen's last sample. It goes out with
                // the tip coming up and the tool still in proximity: libinput
                // ignores axes in the frame that takes the tool out, so with
                // everything in one frame the last stretch of every stroke was
                // cut by a sample.
                self.emit(EV_ABS, ABS_X, x)?;
                self.emit(EV_ABS, ABS_Y, y)?;
                self.emit(EV_ABS, ABS_TILT_X, tilt_x)?;
                self.emit(EV_ABS, ABS_TILT_Y, tilt_y)?;
                self.emit(EV_KEY, BTN_TOUCH, 0)?;
                self.emit(EV_ABS, ABS_PRESSURE, 0)?;
                self.syn()?;

                self.emit(EV_KEY, tool, 0)?;
                self.syn()?;
            }
            2 => {
                // MOVE (pressing)
                self.emit(EV_ABS, ABS_X, x)?;
                self.emit(EV_ABS, ABS_Y, y)?;
                self.emit(EV_ABS, ABS_PRESSURE, contact_pressure)?;
                self.emit(EV_ABS, ABS_TILT_X, tilt_x)?;
                self.emit(EV_ABS, ABS_TILT_Y, tilt_y)?;
                self.syn()?;
            }
            3 => {
                // HOVER — pen near screen, cursor follows without clicking.
                // Requires no INPUT_PROP_DIRECT on the device (we removed it)
                // so libinput classifies this as a tablet tool in proximity.
                self.emit(EV_KEY, tool, 1)?;
                self.emit(EV_ABS, ABS_X, x)?;
                self.emit(EV_ABS, ABS_Y, y)?;
                self.emit(EV_ABS, ABS_PRESSURE, 0)?;
                self.emit(EV_ABS, ABS_TILT_X, tilt_x)?;
                self.emit(EV_ABS, ABS_TILT_Y, tilt_y)?;
                self.syn()?;
            }
            4 => {
                // HOVER_EXIT — pen left proximity
                self.emit(EV_KEY, BTN_TOUCH, 0)?;
                self.emit(EV_KEY, BTN_TOOL_PEN, 0)?;
                self.emit(EV_KEY, BTN_TOOL_RUBBER, 0)?;
                self.emit(EV_ABS, ABS_PRESSURE, 0)?;
                self.syn()?;
            }
            5 => {
                // STYLUS BUTTON DOWN (S Pen side button → right-click in GIMP)
                self.emit(EV_KEY, BTN_STYLUS, 1)?;
                self.syn()?;
            }
            6 => {
                // STYLUS BUTTON UP
                self.emit(EV_KEY, BTN_STYLUS, 0)?;
                self.syn()?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Positions of the two virtual fingers for a pinch `dist` apart, turned `angle` radians clockwise
/// on screen (y grows downwards), around the middle of the pad. Returns (x0, x1, (y0, y1)), kept on the pad.
fn pinch_points(dist: f64, angle: f64) -> (i32, i32, (i32, i32)) {
    let r = dist / 2.0;
    let (cx, cy) = (PAD_W as f64 / 2.0, PAD_H as f64 / 2.0);
    let (dx, dy) = (r * angle.cos(), r * angle.sin());
    let cl = |v: f64, max: i32| (v.round() as i32).clamp(0, max - 1);
    (
        cl(cx - dx, PAD_W),
        cl(cx + dx, PAD_W),
        (cl(cy - dy, PAD_H), cl(cy + dy, PAD_H)),
    )
}

/// The two synthetic fingers of a pinch in progress.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pinch {
    pub(super) dist: f64,
    pub(super) angle: f64,
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

/// The pair of virtual input devices backing one tablet connection.
pub(super) struct InjectDevices {
    pub(super) touch: Option<UInputDevice>,
    pub(super) pen: Option<UInputDevice>,
    /// Takes over the cursor when the pen leaves proximity, so it stays where
    /// the user last pointed instead of vanishing.
    pub(super) pointer: Option<UInputDevice>,
    /// Relative mouse for touchpad mode, made on first use.
    pub(super) mouse: Option<UInputDevice>,
    pub(super) mouse_name: String,
    pub(super) mouse_product: u16,
    /// Keyboard for key combinations, made on first use.
    pub(super) keyboard: Option<UInputDevice>,
    pub(super) keyboard_name: String,
    pub(super) keyboard_product: u16,
    /// Multi-touch pad for native pinch / rotate, made on first use.
    pub(super) gestures: Option<UInputDevice>,
    pub(super) gestures_name: String,
    pub(super) gestures_product: u16,
    /// Set when creating the gesture pad failed, so it is not retried on every message.
    pub(super) gestures_unavailable: bool,
    pub(super) pinch: Option<Pinch>,
    /// Counts pinches, so every set of virtual fingers gets fresh tracking ids.
    pub(super) pinch_count: i32,
    /// Stylus mode: the relative mouse and the gesture pad must not exist, and a profile's clicks
    /// and scrolls go through the absolute pointer instead.
    pub(super) stylus_mode: bool,
    /// Per-connection stylus state: air gestures and pen roll.
    pub(super) stylus: crate::stylus_profile::StylusTracker,
    /// The primary stylus button went down and a profile consumed it, so its release must not
    /// reach the virtual pen either.
    pub(super) pen_button_consumed: bool,
    /// Same for the second stylus button.
    pub(super) pen_button2_consumed: bool,
    /// Mouse buttons held down (bit 0 left, 1 right, 2 middle), so a dropped connection releases
    /// them.
    pub(super) mouse_buttons: u8,
    /// Scroll left over after whole notches were sent, per axis.
    pub(super) scroll_rest: (i32, i32),
    /// Sub-pixel remainder of pointer movement.
    pub(super) rel_rest: (f64, f64),
    pub(super) last_pen_pos: (i32, i32),
    /// Where the output the pen addresses sits on the desktop, as fractions
    /// of the whole desktop: (x, y, width, height).
    ///
    /// The pen is a tablet tool and KWin puts it inside the output named in
    /// `outputName`. The plain absolute pointer is not a tablet tool, and
    /// KWin spreads it across the entire desktop whatever that property
    /// says — so parking it at the pen's own coordinates dropped the cursor
    /// somewhere else entirely, further left and lower the larger the rest
    /// of the desktop was (#18). Converting here puts it where the pen was.
    pub(super) pointer_area: Option<(f64, f64, f64, f64)>,
    /// Bitmask of MT slots that currently have an active tracking ID
    /// (DOWN received, no matching UP yet). Bit N → slot N, up to slot 15.
    pub(super) active_slots: u16,
    pub(super) pen_proximity: bool,
    /// S Pen side button currently held (BTN_STYLUS). Tracked so a held
    /// button is released cleanly if the connection drops.
    pub(super) pen_button: bool,
}

impl InjectDevices {
    /// No devices yet: pen and touch are absent, the mouse and keyboard are made on first use.
    pub(super) fn headless() -> Self {
        Self {
            touch: None,
            pen: None,
            pointer: None,
            mouse: None,
            mouse_name: TOUCHPAD_DEVICE_NAME.into(),
            mouse_product: PRODUCT_TOUCHPAD,
            keyboard: None,
            keyboard_name: KEYBOARD_DEVICE_NAME.into(),
            keyboard_product: PRODUCT_KEYBOARD,
            gestures: None,
            gestures_name: GESTURES_DEVICE_NAME.into(),
            gestures_product: PRODUCT_GESTURES,
            gestures_unavailable: false,
            pinch: None,
            pinch_count: 0,
            stylus_mode: false,
            stylus: Default::default(),
            pen_button_consumed: false,
            pen_button2_consumed: false,
            mouse_buttons: 0,
            scroll_rest: (0, 0),
            rel_rest: (0.0, 0.0),
            last_pen_pos: (0, 0),
            pointer_area: None,
            active_slots: 0,
            pen_proximity: false,
            pen_button: false,
        }
    }

    /// Release all active contacts cleanly before the connection closes.
    /// Without this, a stuck MT slot or a pen left in proximity causes
    /// the next connection to inherit phantom input events.
    pub(super) fn release_all(&mut self) {
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
        if self.pinch.take().is_some() {
            if let Some(ref mut dev) = self.gestures {
                let _ = dev.pad_end();
            }
        }
        self.pen_button_consumed = false;
        self.pen_button2_consumed = false;
        self.stylus.pen_gone();
        self.rel_rest = (0.0, 0.0);
        self.scroll_rest = (0, 0);
    }

    /// The keyboard, created on first use. `None` when uinput is not available.
    pub(super) fn keyboard(&mut self) -> Option<&mut UInputDevice> {
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

    /// Enter or leave Stylus mode. Entering it destroys the relative mouse and the gesture pad (and
    /// ends any pinch): in Stylus mode only the pen and the absolute pointer exist, so a second
    /// pointing device can never fight the pen for the cursor.
    pub(super) fn set_stylus_mode(&mut self, on: bool) {
        self.stylus_mode = on;
        if on {
            // Both taken, never short-circuited: each drop destroys its device.
            let had_mouse = self.mouse.take().is_some();
            let had_pad = self.gestures.take().is_some();
            if had_mouse || had_pad {
                info!("Stylus mode: relative mouse and gesture pad removed");
            }
            self.pinch = None;
            self.mouse_buttons = 0;
            self.rel_rest = (0.0, 0.0);
            self.scroll_rest = (0, 0);
        }
    }

    /// Create the virtual devices the current mode will need now, so the first click or key of a
    /// gesture is not lost to the compositor still discovering a device made at that moment.
    /// Touchpad mode wants the relative mouse, the gesture pad and the keyboard (workspace taps);
    /// Stylus mode only the keyboard (profile key combinations); Display mode none of them.
    pub(super) fn ensure_devices_for(&mut self, mode: crate::config::InputMode) {
        use crate::config::InputMode;
        match mode {
            InputMode::Touchpad => {
                self.set_stylus_mode(false);
                self.mouse();
                self.gesture_pad();
                self.keyboard();
            }
            InputMode::Stylus => {
                self.keyboard();
            }
            InputMode::Display => {}
        }
    }

    /// Put the absolute pointer where the pen last was, so it can click there.
    pub(super) fn park_pointer(&mut self) {
        let (px, py) = self.last_pen_pos;
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
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_ABS, ABS_X, px.clamp(0, COORD_MAX));
            let _ = dev.emit(EV_ABS, ABS_Y, py.clamp(0, COORD_MAX));
            let _ = dev.syn();
        }
    }

    /// The gesture pad, created on first use. `None` if it cannot be created.
    fn gesture_pad(&mut self) -> Option<&mut UInputDevice> {
        if self.gestures.is_none() && !self.gestures_unavailable {
            match UInputDevice::new_gesture_pad(&self.gestures_name, self.gestures_product) {
                Ok(dev) => self.gestures = Some(dev),
                Err(e) => {
                    warn!("No gesture pad: {}. Pinch falls back to Ctrl + wheel.", e);
                    self.gestures_unavailable = true;
                }
            }
        }
        self.gestures.as_mut()
    }

    /// Replay a tablet pinch / rotate step as two fingers on the gesture pad, so the compositor and
    /// the application get a native pinch gesture (libinput `GESTURE_PINCH_*`: scale and angle).
    /// Without the pad, zoom falls back to Ctrl + wheel and rotation is dropped.
    pub(super) fn transform(&mut self, zoom: f64, rotation_deg: f64) {
        if self.stylus_mode || !zoom.is_finite() || !rotation_deg.is_finite() {
            return;
        }
        let zoom = zoom.clamp(0.5, 2.0);
        if self.gesture_pad().is_none() {
            self.scroll_with_modifier(
                crate::stylus_profile::Modifier::Ctrl.code(),
                (zoom.ln() * FALLBACK_UNITS_PER_LN) as i32,
            );
            return;
        }
        let base = self.pinch_count * 2;
        let mut p = match self.pinch {
            Some(p) => p,
            None => {
                self.pinch_count = (self.pinch_count + 1) % 30000;
                if let Some(dev) = self.gestures.as_mut() {
                    let _ = dev.pad_begin(base);
                }
                Pinch {
                    dist: PINCH_START,
                    angle: 0.0,
                }
            }
        };
        p.dist *= zoom;
        p.angle += rotation_deg.to_radians();
        if let Some(dev) = self.gestures.as_mut() {
            if !(PINCH_MIN..=PINCH_MAX).contains(&p.dist) {
                // Out of room on the pad: lift and put the fingers down again at the start distance.
                let _ = dev.pad_end();
                self.pinch_count = (self.pinch_count + 1) % 30000;
                let _ = dev.pad_begin(self.pinch_count * 2);
                p.dist = PINCH_START;
            }
            let _ = dev.pad_move(p.dist, p.angle);
        }
        self.pinch = Some(p);
    }

    /// The tablet's two-finger transform ended: lift the virtual fingers.
    pub(super) fn transform_end(&mut self) {
        if self.pinch.take().is_some() {
            if let Some(dev) = self.gestures.as_mut() {
                let _ = dev.pad_end();
            }
        }
    }

    /// Click through the absolute pointer at the pen's last position (Stylus mode has no relative mouse).
    fn pointer_click(&mut self, button: u16) {
        self.park_pointer();
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_KEY, button, 1);
            let _ = dev.syn();
            let _ = dev.emit(EV_KEY, button, 0);
            let _ = dev.syn();
        }
    }

    /// Carry out a stylus-profile action: click, one wheel notch, or a key combination.
    pub(super) fn run_action(&mut self, action: &crate::stylus_profile::HostAction) {
        use crate::stylus_profile::{HostAction, MouseButton, ScrollDir};
        match action {
            HostAction::MouseClick(b) if self.stylus_mode => self.pointer_click(match b {
                MouseButton::Left => BTN_LEFT,
                MouseButton::Right => BTN_RIGHT,
                MouseButton::Middle => BTN_MIDDLE,
            }),
            HostAction::Scroll(d) if self.stylus_mode => {
                let (dx, dy) = match d {
                    ScrollDir::Up => (0, 120),
                    ScrollDir::Down => (0, -120),
                    ScrollDir::Left => (-120, 0),
                    ScrollDir::Right => (120, 0),
                };
                self.park_pointer();
                let mut rest = self.scroll_rest;
                if let Some(ref mut dev) = self.pointer {
                    let _ = dev.scroll_by(dx, dy, &mut rest);
                }
                self.scroll_rest = rest;
            }
            HostAction::MouseClick(b) => {
                let n = match b {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                };
                if let Some(dev) = self.mouse() {
                    let _ = dev.press(n, true);
                    let _ = dev.press(n, false);
                }
            }
            HostAction::Scroll(d) => {
                let (dx, dy) = match d {
                    ScrollDir::Up => (0, 120),
                    ScrollDir::Down => (0, -120),
                    ScrollDir::Left => (-120, 0),
                    ScrollDir::Right => (120, 0),
                };
                let mut rest = self.scroll_rest;
                if let Some(dev) = self.mouse() {
                    let _ = dev.scroll_by(dx, dy, &mut rest);
                }
                self.scroll_rest = rest;
            }
            HostAction::KeyPress { .. } => {
                let codes = action.key_codes();
                if let Some(dev) = self.keyboard() {
                    if let Err(e) = dev.tap_keys(&codes) {
                        warn!("Failed to send keys: {}", e);
                    }
                }
            }
        }
    }

    /// Scroll with a modifier held (Ctrl for zoom, Alt for rotate): the modifier goes down, the
    /// wheel turns, the modifier comes up, so a lost connection can never leave it stuck.
    pub(super) fn scroll_with_modifier(&mut self, modifier: u16, dy: i32) {
        if dy == 0 {
            return;
        }
        let Some(kb) = self.keyboard() else { return };
        let _ = kb.emit(EV_KEY, modifier, 1);
        let _ = kb.syn();
        let mut rest = self.scroll_rest;
        if let Some(dev) = self.mouse() {
            let _ = dev.scroll_by(0, dy, &mut rest);
        }
        self.scroll_rest = rest;
        if let Some(kb) = self.keyboard.as_mut() {
            let _ = kb.emit(EV_KEY, modifier, 0);
            let _ = kb.syn();
        }
    }

    /// The touchpad-mode mouse, created on first use. `None` when uinput is not available, in
    /// which case the events are dropped (and logged once, by the failed creation).
    pub(super) fn mouse(&mut self) -> Option<&mut UInputDevice> {
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
