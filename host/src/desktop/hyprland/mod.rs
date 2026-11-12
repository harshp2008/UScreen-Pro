//! Hyprland. No `DesktopBackend` yet; `display` enables, places and maps the virtual monitor with `hyprctl`.
// PLATFORM(hyprland): Hyprland has display handling only; focus and the pen menu use the generic backend

pub mod display;
pub mod scale;
