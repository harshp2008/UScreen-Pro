//! Linux evdev / uinput constants the virtual devices use (event types, key and axis codes, ioctl
//! numbers), the device names and product ids, and the axis ranges (coordinates, pen pressure).
//!
//! Kept in one place so the numbers can be checked against `linux/input-event-codes.h` and
//! `linux/uinput.h` without reading any logic.
// PLATFORM(linux): Linux input-event-codes.h values (EV_*, BTN_*, ABS_*)

// Linux input event constants
pub(in crate::input) const EV_SYN: u16 = 0x00;
pub(in crate::input) const EV_KEY: u16 = 0x01;
pub(in crate::input) const EV_REL: u16 = 0x02;
pub(in crate::input) const EV_ABS: u16 = 0x03;

pub(in crate::input) const SYN_REPORT: u16 = 0x00;

pub(in crate::input) const BTN_TOUCH: u16 = 0x14a;
pub(in crate::input) const BTN_TOOL_FINGER: u16 = 0x145;
pub(in crate::input) const BTN_TOOL_QUINTTAP: u16 = 0x148;
pub(in crate::input) const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
pub(in crate::input) const BTN_TOOL_TRIPLETAP: u16 = 0x14e;
pub(in crate::input) const BTN_TOOL_QUADTAP: u16 = 0x14f;
pub(in crate::input) const BTN_TOOL_PEN: u16 = 0x140;
pub(in crate::input) const BTN_TOOL_RUBBER: u16 = 0x141;

pub(in crate::input) const ABS_X: u16 = 0x00;
pub(in crate::input) const ABS_Y: u16 = 0x01;
pub(in crate::input) const ABS_PRESSURE: u16 = 0x18;
pub(in crate::input) const ABS_MT_SLOT: u16 = 0x2f;
pub(in crate::input) const ABS_MT_POSITION_X: u16 = 0x35;
pub(in crate::input) const ABS_MT_POSITION_Y: u16 = 0x36;
pub(in crate::input) const ABS_MT_TRACKING_ID: u16 = 0x39;
pub(in crate::input) const ABS_MT_PRESSURE: u16 = 0x3a;
pub(in crate::input) const ABS_TILT_X: u16 = 0x1a;
pub(in crate::input) const ABS_TILT_Y: u16 = 0x1b;

pub(in crate::input) const BTN_STYLUS: u16 = 0x14b;
pub(in crate::input) const BTN_LEFT: u16 = 0x110;
pub(in crate::input) const BTN_RIGHT: u16 = 0x111;
pub(in crate::input) const BTN_MIDDLE: u16 = 0x112;

pub(in crate::input) const KEY_LEFTCTRL: u16 = 29;
pub(in crate::input) const KEY_LEFTSHIFT: u16 = 42;
pub(in crate::input) const KEY_LEFTMETA: u16 = 125;
pub(in crate::input) const KEY_A: u16 = 30;
pub(in crate::input) const KEY_Z: u16 = 44;
pub(in crate::input) const KEY_X: u16 = 45;
pub(in crate::input) const KEY_C: u16 = 46;
pub(in crate::input) const KEY_V: u16 = 47;
pub(in crate::input) const KEY_PAGEUP: u16 = 104;
pub(in crate::input) const KEY_PAGEDOWN: u16 = 109;

pub(in crate::input) const REL_X: u16 = 0x00;
pub(in crate::input) const REL_Y: u16 = 0x01;
pub(in crate::input) const REL_HWHEEL: u16 = 0x06;
pub(in crate::input) const REL_WHEEL: u16 = 0x08;
pub(in crate::input) const REL_WHEEL_HI_RES: u16 = 0x0b;
pub(in crate::input) const REL_HWHEEL_HI_RES: u16 = 0x0c;

// uinput ioctl constants (modern UI_DEV_SETUP/UI_ABS_SETUP API — the legacy
// uinput_user_dev write() API cannot declare axis resolution, which makes
// libinput reject the device: "missing tablet capabilities ... resolution")
pub(in crate::input) const UI_SET_EVBIT: libc::c_ulong = 0x40045564;
pub(in crate::input) const UI_SET_KEYBIT: libc::c_ulong = 0x40045565;
pub(in crate::input) const UI_SET_RELBIT: libc::c_ulong = 0x40045566;
pub(in crate::input) const UI_SET_ABSBIT: libc::c_ulong = 0x40045567;
pub(in crate::input) const UI_SET_PROPBIT: libc::c_ulong = 0x4004556e;
pub(in crate::input) const UI_DEV_SETUP: libc::c_ulong = 0x405c5503;
pub(in crate::input) const UI_ABS_SETUP: libc::c_ulong = 0x401c5504;
pub(in crate::input) const UI_DEV_CREATE: libc::c_ulong = 0x5501;
pub(in crate::input) const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

pub(in crate::input) const BUS_VIRTUAL: u16 = 0x06;
pub(in crate::input) const INPUT_PROP_DIRECT: i32 = 0x01;
/// "This is a pointing device that needs a cursor" (value 0). Declared on the touchpad-mode mouse
/// so the compositor treats it as an ordinary mouse. Deliberately not set on it: INPUT_PROP_DIRECT
/// (touchscreen), INPUT_PROP_BUTTONPAD and INPUT_PROP_SEMI_MT (touchpad classes).
pub(in crate::input) const INPUT_PROP_POINTER: i32 = 0x00;
/// "The touchpad has no separate buttons, the whole pad clicks" (value 2). Set on the virtual
/// touchpad so libinput treats it like a laptop clickpad.
pub(in crate::input) const INPUT_PROP_BUTTONPAD: i32 = 0x02;

/// Coordinates are injected in a fixed 0..65535 space — the compositor maps
/// the device onto the output, so the virtual display resolution can change
/// at runtime without recreating uinput devices.
pub(in crate::input) const COORD_MAX: i32 = 65535;

/// Identity of the virtual input devices. These must stay stable: KDE keys the
/// device→output association in kcminputrc on vendor/product/name, so changing
/// any of them silently orphans the mapping written by `map_devices_to_output`.
pub(in crate::input) const UINPUT_VENDOR: u16 = 0x4553;
pub(in crate::input) const PRODUCT_TOUCH: u16 = 0x0001;
pub(in crate::input) const PRODUCT_PEN: u16 = 0x0002;
pub(in crate::input) const PRODUCT_POINTER: u16 = 0x0003;
pub(in crate::input) const PRODUCT_TOUCHPAD: u16 = 0x0004;
pub(in crate::input) const PRODUCT_KEYBOARD: u16 = 0x0005;
pub(in crate::input) const PRODUCT_MOUSE: u16 = 0x0006;
pub(in crate::input) const TOUCH_DEVICE_NAME: &str = "UScreen Touch";
pub(in crate::input) const PEN_DEVICE_NAME: &str = "UScreen Pen";
pub(in crate::input) const POINTER_DEVICE_NAME: &str = "UScreen Pointer";
pub(in crate::input) const TOUCHPAD_DEVICE_NAME: &str = "UScreen Touchpad";
pub(in crate::input) const KEYBOARD_DEVICE_NAME: &str = "UScreen Keyboard";
/// The plain relative mouse (right-click zone, wake nudge, and the whole of touchpad mode when
/// native gestures are off). The multi-touch pad owns the name "UScreen Touchpad".
pub(in crate::input) const MOUSE_DEVICE_NAME: &str = "UScreen Mouse";
/// ~310mm wide active area → 65535/310 ≈ 211 units/mm.
/// libinput requires a resolution on touchscreen/tablet axes.
pub(in crate::input) const RESOLUTION_UNITS_PER_MM: i32 = 211;
/// The virtual pen's pressure axis runs 0..=PEN_PRESSURE_MAX.
pub(in crate::input) const PEN_PRESSURE_MAX: i32 = 4096;
