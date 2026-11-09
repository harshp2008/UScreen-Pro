//! The virtual monitor itself, independent of any desktop.
//!
//! - [`edid`] is the generated EDID the kernel's EVDI driver is given.
//! - [`vdisplay`] finds which DRM cards and connectors belong to EVDI, through sysfs.
//!
//! How a given desktop enables, places and maps that monitor is in `desktop/<name>/display.rs`,
//! because each compositor exposes a different (and differently buggy) interface for the same
//! operations.

pub mod edid;
pub mod vdisplay;
