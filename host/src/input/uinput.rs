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
pub(super) const BTN_TOOL_QUINTTAP: u16 = 0x148;
pub(super) const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
pub(super) const BTN_TOOL_TRIPLETAP: u16 = 0x14e;
pub(super) const BTN_TOOL_QUADTAP: u16 = 0x14f;
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

pub(super) const KEY_LEFTCTRL: u16 = 29;
pub(super) const KEY_LEFTMETA: u16 = 125;
pub(super) const KEY_PAGEUP: u16 = 104;
pub(super) const KEY_PAGEDOWN: u16 = 109;

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
/// "This is a pointing device that needs a cursor" (value 0). Declared on the touchpad-mode mouse
/// so the compositor treats it as an ordinary mouse. Deliberately not set on it: INPUT_PROP_DIRECT
/// (touchscreen), INPUT_PROP_BUTTONPAD and INPUT_PROP_SEMI_MT (touchpad classes).
pub(super) const INPUT_PROP_POINTER: i32 = 0x00;
/// "The touchpad has no separate buttons, the whole pad clicks" (value 2). Set on the virtual
/// touchpad so libinput treats it like a laptop clickpad.
pub(super) const INPUT_PROP_BUTTONPAD: i32 = 0x02;

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
pub(super) const PRODUCT_MOUSE: u16 = 0x0006;
pub(super) const TOUCH_DEVICE_NAME: &str = "UScreen Touch";
pub(super) const PEN_DEVICE_NAME: &str = "UScreen Pen";
pub(super) const POINTER_DEVICE_NAME: &str = "UScreen Pointer";
pub(super) const TOUCHPAD_DEVICE_NAME: &str = "UScreen Touchpad";
pub(super) const KEYBOARD_DEVICE_NAME: &str = "UScreen Keyboard";
/// The plain relative mouse (right-click zone, wake nudge, and the whole of touchpad mode when
/// native gestures are off). The multi-touch pad owns the name "UScreen Touchpad".
pub(super) const MOUSE_DEVICE_NAME: &str = "UScreen Mouse";
/// Device names and product ids for the N-th tablet. The first keeps the
/// original names and ids so existing KWin input mappings stay valid; every
/// further one gets a numbered name and its own product range, since KWin
/// keys its per-device settings on vendor/product/name.
#[derive(Clone, Debug)]
pub struct DeviceIdentity {
    pub touch: String,
    pub pen: String,
    pub pointer: String,
    /// The virtual multi-touch touchpad (`UScreen Touchpad`) that native gestures are replayed on.
    /// Only made while the input mode is Touchpad, and never mapped to an output.
    pub touchpad: String,
    /// Plain relative mouse used for clicks the pad cannot make and by touchpad mode without
    /// native gestures. Created on first use and never mapped to an output.
    pub mouse: String,
    /// Virtual keyboard for touchpad gestures (workspace switch, zoom modifier). Created on
    /// first use.
    pub keyboard: String,
    pub product_touch: u16,
    pub product_pen: u16,
    pub product_pointer: u16,
    pub product_touchpad: u16,
    pub product_mouse: u16,
    pub product_keyboard: u16,
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
            mouse: format!("{}{}", MOUSE_DEVICE_NAME, suffix),
            keyboard: format!("{}{}", KEYBOARD_DEVICE_NAME, suffix),
            product_touch: PRODUCT_TOUCH + base,
            product_pen: PRODUCT_PEN + base,
            product_pointer: PRODUCT_POINTER + base,
            product_touchpad: PRODUCT_TOUCHPAD + base,
            product_mouse: PRODUCT_MOUSE + base,
            product_keyboard: PRODUCT_KEYBOARD + base,
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

/// The virtual touchpad's surface, in the physical size libinput reasons about: 640 x 420 mm at
/// 40 units/mm. The tablet centres its touch area on the pad and scales finger positions by the
/// sensitivity setting (up to 2x), so even the largest tablet at the highest sensitivity stays
/// clear of libinput's edge-palm zones and the clickpad button area along the pad's border.
pub(super) const PAD_UNITS_PER_MM: i32 = 40;
pub(super) const PAD_WIDTH_MM: f64 = 640.0;
pub(super) const PAD_HEIGHT_MM: f64 = 420.0;
/// Contacts the pad can hold at once (slots 0..PAD_SLOTS).
pub(super) const PAD_SLOTS: usize = 10;
/// ABS_MT_PRESSURE runs 0..=PAD_PRESSURE_MAX. Android pressure is a 0..1 estimate that is often
/// tiny; libinput drops contacts below its "touching" threshold and treats very high ones as palms,
/// so the tablet's value is squeezed into a band that is always a plain finger.
pub(super) const PAD_PRESSURE_MAX: i32 = 255;
const PAD_PRESSURE_LOW: f64 = 50.0;
const PAD_PRESSURE_SPAN: f64 = 40.0;

/// One finger on the tablet's touchpad, as the host receives it: Android pointer id and a
/// position in millimetres from the touch area's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PadFinger {
    pub(super) id: i32,
    pub(super) x_mm: f64,
    pub(super) y_mm: f64,
    /// Android's 0..1 pressure estimate.
    pub(super) pressure: f64,
}

/// What the virtual touchpad has been told so far: which Android pointer id sits in which MT slot,
/// the next tracking id, and which `BTN_TOOL_*` finger-count code is currently set.
#[derive(Debug)]
pub(super) struct PadState {
    pub(super) slots: [Option<i32>; PAD_SLOTS],
    pub(super) next_tracking: i32,
    pub(super) tool: Option<u16>,
}

impl PadState {
    pub(super) fn new() -> Self {
        Self {
            slots: [None; PAD_SLOTS],
            next_tracking: 0,
            tool: None,
        }
    }
    pub(super) fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

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
        Ok(Self { file })
    }

    /// A multi-touch touchpad, like the clickpad of a laptop. The tablet's raw fingers are replayed
    /// on it and libinput does the rest: pointer motion, taps, two-finger scroll, pinch, and 3 / 4
    /// finger swipes, which GNOME and KDE turn into workspace and overview gestures.
    ///
    /// Declared the way a real clickpad is: MT protocol B slots with tracking ids, finger-count tool
    /// buttons (`BTN_TOOL_FINGER` .. `BTN_TOOL_QUINTTAP`), a pressure axis, a real resolution (libinput
    /// measures every threshold in millimetres), INPUT_PROP_POINTER and INPUT_PROP_BUTTONPAD. Not
    /// INPUT_PROP_DIRECT: that is a touchscreen and would put the finger on the screen.
    pub(super) fn new_touchpad(name: &str, product: u16) -> Result<Self> {
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
        Ok(Self { file })
    }

    /// Put the tablet's fingers on the pad: the full set that is down right now. Fingers that are
    /// no longer listed are lifted (tracking id -1), new ones get the lowest free slot and a fresh
    /// tracking id, the rest just move. The finger-count tool button follows the count and the
    /// frame ends with one SYN_REPORT, the way a real pad reports.
    ///
    /// Sending the whole set every time (instead of down / move / up events) means a lost message
    /// can never leave a finger stuck on the pad: the next frame simply does not list it.
    pub(super) fn inject_pad(&mut self, st: &mut PadState, fingers: &[PadFinger]) -> Result<()> {
        if fingers.is_empty() && st.count() == 0 {
            return Ok(());
        }
        // Lift what is gone.
        for slot in 0..PAD_SLOTS {
            if let Some(id) = st.slots[slot] {
                if !fingers.iter().any(|f| f.id == id) {
                    self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
                    self.emit(EV_ABS, ABS_MT_TRACKING_ID, -1)?;
                    st.slots[slot] = None;
                }
            }
        }
        // Place and move what is down.
        let mut first: Option<(i32, i32)> = None;
        for f in fingers {
            let (slot, is_new) = match st.slots.iter().position(|s| *s == Some(f.id)) {
                Some(s) => (s, false),
                None => match st.slots.iter().position(|s| s.is_none()) {
                    Some(s) => {
                        st.slots[s] = Some(f.id);
                        (s, true)
                    }
                    None => continue, // more fingers than the pad has slots
                },
            };
            let x = ((f.x_mm * PAD_UNITS_PER_MM as f64) as i32)
                .clamp(0, (PAD_WIDTH_MM * PAD_UNITS_PER_MM as f64) as i32 - 1);
            let y = ((f.y_mm * PAD_UNITS_PER_MM as f64) as i32)
                .clamp(0, (PAD_HEIGHT_MM * PAD_UNITS_PER_MM as f64) as i32 - 1);
            let pressure = (PAD_PRESSURE_LOW + f.pressure.clamp(0.0, 1.0) * PAD_PRESSURE_SPAN) as i32;
            self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
            if is_new {
                self.emit(EV_ABS, ABS_MT_TRACKING_ID, st.next_tracking)?;
                st.next_tracking = (st.next_tracking + 1) & 0xffff;
            }
            self.emit(EV_ABS, ABS_MT_POSITION_X, x)?;
            self.emit(EV_ABS, ABS_MT_POSITION_Y, y)?;
            self.emit(EV_ABS, ABS_MT_PRESSURE, pressure)?;
            if first.is_none() {
                first = Some((x, y));
            }
        }
        // Single-touch mirror of the first finger, and the finger-count buttons.
        let count = st.count();
        if let Some((x, y)) = first {
            self.emit(EV_ABS, ABS_X, x)?;
            self.emit(EV_ABS, ABS_Y, y)?;
        }
        let tool = match count {
            0 => None,
            1 => Some(BTN_TOOL_FINGER),
            2 => Some(BTN_TOOL_DOUBLETAP),
            3 => Some(BTN_TOOL_TRIPLETAP),
            4 => Some(BTN_TOOL_QUADTAP),
            _ => Some(BTN_TOOL_QUINTTAP),
        };
        if tool != st.tool {
            if let Some(old) = st.tool {
                self.emit(EV_KEY, old, 0)?;
            }
            if let Some(new) = tool {
                self.emit(EV_KEY, new, 1)?;
            }
            if st.tool.is_none() || tool.is_none() {
                self.emit(EV_KEY, BTN_TOUCH, tool.is_some() as i32)?;
            }
            st.tool = tool;
        }
        self.syn()
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

    /// A plain keyboard, made the first time a touchpad gesture needs a key.
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
    /// The multi-touch touchpad native gestures are replayed on. Exists only while the input mode
    /// is Touchpad and native gestures are in use.
    pub(super) pad: Option<UInputDevice>,
    pub(super) pad_name: String,
    pub(super) pad_product: u16,
    pub(super) pad_state: PadState,
    /// Keyboard for key combinations, made on first use.
    pub(super) keyboard: Option<UInputDevice>,
    pub(super) keyboard_name: String,
    pub(super) keyboard_product: u16,
    /// Stylus mode: the relative mouse must not exist.
    pub(super) stylus_mode: bool,
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
    /// Buttons the pen holds on the absolute pointer in `mouse` style (bit 0 left, 1 right,
    /// 2 middle), so a dropped connection lets go of them.
    pub(super) pen_mouse_buttons: u8,
    /// Ctrl is held on the keyboard for a pinch in progress, and when the last pinch step came.
    pub(super) pinch_since: Option<std::time::Instant>,
}

impl InjectDevices {
    /// No devices yet: pen and touch are absent, the mouse and keyboard are made on first use.
    pub(super) fn headless() -> Self {
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
            active_slots: 0,
            pen_proximity: false,
            pen_button: false,
            pen_mouse_buttons: 0,
            pinch_since: None,
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
        self.end_pinch();
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

    /// Enter or leave Stylus mode. Entering it destroys the relative mouse: in Stylus mode only the
    /// pen and the absolute pointer exist, so a second pointing device can never fight the pen for
    /// the cursor.
    pub(super) fn set_stylus_mode(&mut self, on: bool) {
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
    pub(super) fn ensure_devices_for(&mut self, mode: crate::config::InputMode, native: bool) {
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
    pub(super) fn pad(&mut self) -> Option<&mut UInputDevice> {
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
                    warn!("No multi-touch touchpad: {}. Native gestures will do nothing.", e);
                    return None;
                }
            }
        }
        self.pad.as_mut()
    }

    /// Replay the tablet's fingers on the touchpad (see [`UInputDevice::inject_pad`]).
    pub(super) fn touchpad_raw(&mut self, fingers: &[PadFinger]) {
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
    pub(super) fn release_pad(&mut self) {
        if self.pad_state.count() > 0 {
            self.touchpad_raw(&[]);
        }
    }

    /// Remove the touchpad (leaving Touchpad mode, or native gestures turned off).
    pub(super) fn drop_pad(&mut self) {
        if self.pad.is_some() {
            self.release_pad();
            self.pad = None;
            info!("Touchpad mode left: multi-touch touchpad removed");
        }
        self.pad_state = PadState::new();
    }

    /// Pen coordinates (0..=COORD_MAX over the pen's output) as pointer coordinates over the whole
    /// desktop.
    fn desktop_xy(&self, px: i32, py: i32) -> (i32, i32) {
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
        (px.clamp(0, COORD_MAX), py.clamp(0, COORD_MAX))
    }

    /// Put the absolute pointer where the pen last was, so it can click there.
    pub(super) fn park_pointer(&mut self) {
        let (px, py) = self.desktop_xy(self.last_pen_pos.0, self.last_pen_pos.1);
        if let Some(ref mut dev) = self.pointer {
            let _ = dev.emit(EV_ABS, ABS_X, px);
            let _ = dev.emit(EV_ABS, ABS_Y, py);
            let _ = dev.syn();
        }
    }

    /// Whether the pen can act as a mouse: needs the absolute pointer.
    pub(super) fn has_pointer(&self) -> bool {
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
    pub(super) fn pen_as_mouse(
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

    /// Let go of every button the pen holds on the absolute pointer.
    pub(super) fn release_pen_mouse(&mut self) {
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
    pub(super) fn tap_key_combo(&mut self, codes: &[u16]) {
        if let Some(dev) = self.keyboard() {
            if let Err(e) = dev.tap_keys(codes) {
                warn!("Failed to send keys: {}", e);
            }
        }
    }

    /// One step of a pinch: Ctrl is pressed with the first step and stays down until
    /// [`Self::end_pinch`], and the wheel turns under it. Pressing and releasing Ctrl around every
    /// step lost it to the order the compositor reads two devices in.
    pub(super) fn pinch_step(&mut self, dy: i32) {
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
    pub(super) fn end_pinch(&mut self) {
        if self.pinch_since.take().is_some() {
            if let Some(kb) = self.keyboard.as_mut() {
                let _ = kb.emit(EV_KEY, KEY_LEFTCTRL, 0);
                let _ = kb.syn();
            }
        }
    }

    /// Safety net: Ctrl held for a pinch whose end never arrived is let go after this long.
    pub(super) fn expire_pinch(&mut self) {
        if self.pinch_since.is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(1500)) {
            self.end_pinch();
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
