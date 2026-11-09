---
title: "Unsupported desktops"
---

For a desktop without a backend, the `generic` backend is used. It reports no capabilities except reading a focus file that a helper script may write (`~/.local/share/uscreen/focus.json`). So:

| Feature | Without a backend |
| :--- | :--- |
| Extended Display | works where EVDI works; the output is not placed for you |
| Pen and touch mapping | manual |
| Dynamic pen style per app | no: the pen stays on one route |
| Pen menu | no |

Hyprland and COSMIC have code that enables the virtual output (`hyprctl`, `cosmic-randr`); both are **not verified** here.

## Sources

- `host/src/desktop/generic/mod.rs`, `host/src/desktop/hyprland/`, `host/src/desktop/cosmic/`
- `docs/compatibility.md`
