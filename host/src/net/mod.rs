//! Connections between the host and the tablet.
//!
//! - [`adb`]: device discovery, `adb reverse` port forwarding, app launching and the monitor loop.
//! - [`pairing`]: wireless pairing and the legacy `adb tcpip` fallback.
//! - [`installer`]: the queue that installs the bundled APK on tablets.
//! - [`stream`]: the loopback video socket.
//! - [`notices`]: one-shot status messages pushed to the tablet over the input socket.
//!
//! The input socket itself lives in [`crate::input`] because it owns the uinput devices.

pub mod adb;
pub mod installer;
pub mod notices;
pub mod pairing;
pub mod stream;
