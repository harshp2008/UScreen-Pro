# Backlog

Things we agreed to develop later, roughly by area. Nothing here is started unless it says so.

## Testing and verification
- **Camera rig for the ghost cursor.** A phone on a stand photographs the laptop screen over `adb`; a vision model answers "is there a second cursor?" (opt-in, nothing leaves the machine unless the script is run). Accuracy on a faint arrow is untested. The software-only scanout capture was dropped (experiment 103).
- **Headless real-compositor tests in CI:** run GNOME's compositor headless to test the Shell extension and the pen menu without a person.
- **Nightly VM matrix** per distribution for the EVDI kernel-module build (slow, so not per commit).
- **Installer: ask the running extension for its loaded version,** so a re-run after an install without log out still says a log out is needed.
- Test the migrated release, packaging and CI pipeline (written but never run).

## Pen and input
- Tier 2 text-field detection (AT-SPI) so the pen menu stops offering Paste over blank page space.
- A desktop-app control for `pen_click_hold_ms`; a configurable click-zone size; a smooth exit from the click zone (no jump).
- Make a silent pen-style change from the tablet app visible or harder to do.
- Pointer snapping to whole pixels exists for GNOME only; check KDE, Sway and X11 for their own ghost behaviour.

## Display
- Display mode with hardware cursors: the cursor stays on the previous screen (experiment 56). Software cursors are used today.
- Build upstream EVDI for the newest kernels where the distribution's package is too old; watch Ubuntu's fix (bug 2133812).

## Environments
- Backends for KDE, Sway/wlroots and plain X11 beyond "generic"; per-GNOME-version handling in the Shell extension.
- "Dynamic" pen mode (stored as `auto`) is experimental and tested only on GNOME / Ubuntu.

## Installer
- Polish the text interface (progress, per-part status, retry a failed part).
