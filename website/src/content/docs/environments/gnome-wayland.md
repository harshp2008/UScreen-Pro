---
title: "GNOME on Wayland"
---

The platform this project has been run on: **Ubuntu 24.04, GNOME Shell 46, Wayland**. Nothing on this page was checked on another Shell version.

## What works there

| Feature | How | Recorded in |
| :--- | :--- | :--- |
| Extended Display | EVDI virtual display; the daemon enables and places it through Mutter's D-Bus display configuration | the experiments on EVDI and Mutter (for example 56) |
| Pen and touch mapped to the virtual output | GNOME's per-device settings | no separate record |
| Focus reporting for the Dynamic pen style | the `uscreen-app-lock` Shell extension writes `focus.json` | experiments 95 to 98 |
| Pen menu | drawn by the extension inside the Shell; fallback window with `spawn-at` | experiment 102 |
| App lock (keep chosen apps off the virtual display) | the extension moves a window to the primary monitor | no separate record |
| Snapping the pointer to whole pixels | GNOME only; avoids stale cursor images with software cursors | experiment 103 and the tests in `host/src/input/ghost/tests/pointer.rs` |

The maintainer runs this platform. Where the last column says "no separate record", nothing in the repository documents a check of that feature.

## What does not

A new or changed Shell extension loads only after a log out and in; the installer reports that. The extension is written for GNOME Shell 45 and later. Other versions are listed in its metadata but only 46 was run; see [Known shortcomings](/UScreen-Pro/shortcomings/).

## Sources

- `host/src/desktop/gnome/` (`mod.rs`, `display.rs`, `spawn_at.rs`)
- `extensions/uscreen-app-lock/` (`compat/versions.js` for the versions)
- `docs/desktop-backends.md` ("Status", "The GNOME backend in detail")
