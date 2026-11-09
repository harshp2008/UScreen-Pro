//! [`DeviceIdentity`]: the names and product ids of the virtual devices of the N-th tablet, so two
//! tablets never share a device and KWin/libinput settings stay attached to the right one.

use super::*;

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
    pub(in crate::input) fn owns(&self, name: &str) -> bool {
        name == self.touch || name == self.pen || name == self.pointer
    }
}
