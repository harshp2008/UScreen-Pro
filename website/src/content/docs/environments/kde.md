---
title: "KDE Plasma"
---

**Not verified here.** The code is inherited from the project this one continues, which reports KDE Plasma on Wayland as its reference setup. This project did not run it.

What exists in the code: the virtual output is placed and the pen and touch devices are mapped through KWin's D-Bus interface, and the daemon turns the KDE on-screen keyboard off while it runs and restores it on exit. KDE has no `DesktopBackend` yet, so focus reporting for the Dynamic pen style and the pen menu fall back to `generic`.

The installer never asks for a log out on KDE.

## Sources

- `host/src/desktop/kde/` (`display.rs`, `osk.rs`), `host/src/desktop/generic/mod.rs`
- `docs/compatibility.md` (reports from the upstream project)
