//! KDE Plasma. There is no `DesktopBackend` for Plasma yet (the pen menu and focus routing use `generic`);
//! what exists is the virtual-monitor handling through KWin (`display`) and the on-screen-keyboard policy
//! (`osk`).

pub mod display;
pub mod osk;
