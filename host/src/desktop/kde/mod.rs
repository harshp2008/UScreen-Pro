//! KDE Plasma. There is no `DesktopBackend` for Plasma yet (the pen menu and focus routing use `generic`);
//! what exists is the virtual-monitor handling through KWin (`display`) and the on-screen-keyboard policy
//! (`osk`).
// PLATFORM(kde): KDE Plasma has display handling and the keyboard policy only; focus and the pen menu use the generic backend

pub mod display;
pub mod kwin;
pub mod osk;
pub mod scale;
