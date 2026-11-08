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
pub(super) const EV_ABS: u16 = 0x03;

pub(super) const SYN_REPORT: u16 = 0x00;

pub(super) const BTN_TOUCH: u16 = 0x14a;
pub(super) const BTN_TOOL_FINGER: u16 = 0x145;
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

// uinput ioctl constants (modern UI_DEV_SETUP/UI_ABS_SETUP API — the legacy
// uinput_user_dev write() API cannot declare axis resolution, which makes
// libinput reject the device: "missing tablet capabilities ... resolution")
pub(super) const UI_SET_EVBIT: libc::c_ulong = 0x40045564;
pub(super) const UI_SET_KEYBIT: libc::c_ulong = 0x40045565;
pub(super) const UI_SET_ABSBIT: libc::c_ulong = 0x40045567;
pub(super) const UI_SET_PROPBIT: libc::c_ulong = 0x4004556e;
pub(super) const UI_DEV_SETUP: libc::c_ulong = 0x405c5503;
pub(super) const UI_ABS_SETUP: libc::c_ulong = 0x401c5504;
pub(super) const UI_DEV_CREATE: libc::c_ulong = 0x5501;
pub(super) const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

pub(super) const BUS_VIRTUAL: u16 = 0x06;
pub(super) const INPUT_PROP_DIRECT: i32 = 0x01;

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
pub(super) const TOUCH_DEVICE_NAME: &str = "UScreen Touch";
pub(super) const PEN_DEVICE_NAME: &str = "UScreen Pen";
pub(super) const POINTER_DEVICE_NAME: &str = "UScreen Pointer";

/// Device names and product ids for the N-th tablet. The first keeps the
/// original names and ids so existing KWin input mappings stay valid; every
/// further one gets a numbered name and its own product range, since KWin
/// keys its per-device settings on vendor/product/name.
#[derive(Clone, Debug)]
pub struct DeviceIdentity {
    pub touch: String,
    pub pen: String,
    pub pointer: String,
    pub product_touch: u16,
    pub product_pen: u16,
    pub product_pointer: u16,
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
            product_touch: PRODUCT_TOUCH + base,
            product_pen: PRODUCT_PEN + base,
            product_pointer: PRODUCT_POINTER + base,
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
            Self::ioctl_val(fd, UI_SET_KEYBIT, BTN_LEFT as i32)?;
            Self::abs_setup(fd, ABS_X, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::abs_setup(fd, ABS_Y, 0, w - 1, RESOLUTION_UNITS_PER_MM)?;
            Self::dev_setup_and_create(fd, name, product)?;
        }

        info!("uinput pointer '{}' created", name);
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { file })
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
    }
}
