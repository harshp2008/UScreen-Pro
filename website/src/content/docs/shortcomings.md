---
title: "Known shortcomings"
---

Problems and gaps that are known and not fixed. Each is a fact from the code or the records, not a guess.

### Only one platform has been run

Ubuntu 24.04 with GNOME 46 on Wayland. Every other desktop, distribution, package format and tablet is untested here.

### The Dynamic pen style is experimental and GNOME-only

It needs the GNOME Shell extension to know which app has focus. Without it the pen stays on one route; `uscreen toggle-tablet` flips it by hand.

### A changed Shell extension needs a log out on Wayland

GNOME Shell on Wayland loads a new or changed extension only at login. The installer says so in its result line. The extension cannot be tested in a running session without logging out, so its changes are checked only for syntax and for agreement of the version lists.

### The extension names Shell versions it cannot load

`metadata.json` lists GNOME Shell 42 to 47, but the extension is written as an ES module, the format of GNOME 45 and later. Versions 42 to 44 cannot load it. The table in `compat/versions.js` marks them `unloadable`; the list in `metadata.json` was not changed.

### The update notice follows the upstream project

The daily update check asks the release page of the project this one continues, not this repository's.

### A resting real pen may not park

The pen is parked after 300 ms inside 1.5 mm. Only a 1 pixel injected rest was measured; a real S Pen's resting noise is unknown, and the radii are not yet settings.

### Android does not report pen hover distance

So the distance is not used by the hover filter.

### Wi-Fi has more stutter than the cable

The tail latency is worse on Wi-Fi; see the benchmarks.

### Package files were not run

The `.deb`, `.rpm` and Arch files exist in `packaging/`; this project did not build or install them.

## Sources

- `docs/backlog.md`, `docs/benchmarks.md`
- `experiments_and_findings/100_hover_park_and_recorder.md` ("Not verified")
- `extensions/uscreen-app-lock/metadata.json`, `compat/versions.js`
- `host/src/daemon/update.rs`
