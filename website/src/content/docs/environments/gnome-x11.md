---
title: "GNOME on X11"
---

**Not verified.** There is no X11 session test in this repository.

Any X11 session gets a small backend that reads the focused window from the `_NET_ACTIVE_WINDOW` property and its `WM_CLASS`. The virtual monitor is enabled with RandR, and the tablet's pen and touch devices are mapped onto it with `xinput map-to-output`, re-applied after RandR changes. GNOME also has a path that sets its per-device mapping. An X11 session needs no log out for the extension: the Shell can be reloaded with Alt+F2, `r`.

The X11 backend provides only the `focus` capability. It has no pointer context and no pen menu, so the pen menu is not offered there.

## Sources

- `host/src/desktop/x11/` (`mod.rs` focus, `display.rs` RandR and `xinput`)
- `docs/desktop-backends.md` ("Status")
- `docs/installer.md` ("When is a restart needed?")
