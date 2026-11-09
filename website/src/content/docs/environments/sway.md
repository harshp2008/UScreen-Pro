---
title: "Sway"
---

**Not verified, and no integration.** There is no code for Sway or other wlroots compositors beyond the `generic` backend.

The virtual display and the stream work wherever EVDI does. Placing the output and assigning the "UScreen Pen" and "UScreen Touch" devices to it are manual: use your compositor's own configuration. The pen stays on one route; `uscreen toggle-tablet` flips it, and can be bound to a key.

Adding a Sway backend is a new directory under `host/src/desktop/`; the guide is under Project documents.

## Sources

- `host/src/desktop/generic/mod.rs`
- `docs/compatibility.md` ("Other desktops"), `docs/desktop-backends.md`
