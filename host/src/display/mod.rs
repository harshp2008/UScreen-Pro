//! Everything about the virtual screen as the desktop sees it.
//!
//! - [`edid`] and [`vdisplay`] describe the virtual monitor: the generated EDID the kernel's EVDI
//!   driver is given, and the lookup of which DRM cards / connectors belong to EVDI.
//! - [`kwin`], [`gnome`], [`hyprland`], [`cosmic`] and [`x11`] are the per-compositor backends that
//!   enable, place and remove that monitor. They are separate because each compositor exposes a
//!   different (and differently buggy) interface for the same three operations.

pub mod cosmic;
pub mod edid;
pub mod gnome;
pub mod hyprland;
pub mod kwin;
pub mod vdisplay;
pub mod x11;
