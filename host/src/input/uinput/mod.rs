//! Virtual input devices: Linux input / uinput constants, [`DeviceIdentity`] (names and product ids
//! per tablet), and the `UInputDevice` / `InjectDevices` wrappers that create the devices through
//! `/dev/uinput` and write evdev events into them.
//!
//! uinput is used instead of compositor APIs so touch, pen and pointer input works the same on X11,
//! GNOME, KDE and wlroots.
//!
//! Layout: `consts` (numbers), `identity` (names/ids), `pad` (multi-touch pad), `device` (one
//! device), `inject` (touch/pen frames), `devices` (the per-tablet set).
// PLATFORM(uinput): virtual pen, touch and pointer devices created through /dev/uinput (needs the uinput module and permission)

use super::*;

mod consts;
mod device;
mod devices;
mod identity;
mod inject;
mod pad;

pub(in crate::input) use consts::*;
pub(in crate::input) use device::*;
pub(in crate::input) use devices::*;
pub use identity::*;
pub(in crate::input) use pad::*;
