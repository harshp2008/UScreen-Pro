---
title: "102 Codebase cleanup, the desktop layer, and the pen's long-press menu"
sidebar:
  order: 61
  label: "102 Codebase cleanup, the desktop layer, and the pen's long-"
---

Three pieces of work in one pass: (0) cleanup of the host, (1/4) a desktop layer so features do not hardcode GNOME,
(2/3) the long-press menu (cut / copy / paste / select all / undo) built on it.

| Field | Value |
| :-- | :-- |
| **ID** | 102 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Cleanup (no behaviour change; 169 tests and the live checks were identical before and after)

* Top-level modules grouped: `config/` (+`handler`, `options/`), `daemon/` (+`session/`, `guard/`, `runtime`, `tray`, `update`, `osk`),
  `cli/` (+`commands`, `doctor/`), `capture/` (+`encoder(s)`, `latency`, `timing`).
* Large files split into folders by concern: `uinput/` (consts, identity, pad, device, inject, devices), `input/ghost/` (route, pointer,
  auto, background, control + tests), `input/events/` (one handler per message kind + tests), `protocol/seq_filter`, `doctor/`,
  `session/`, `config/` (limits, curves, modes, profile, clean), `guard/`, `options/`. The biggest file is now 716 lines (`adb_monitor`,
  one long function, left as it is).
* Dead code: the compiler reported none; one unused constant (`TIMING_VERSION`) removed. Candidates **not** removed (tracked, referenced by
  packaging/CI, or yours to decide): `gui-legacy/`, `scratch/`, `upstream_prep/`, `scripts/deploy_fixes.sh` (hardcodes a home path and the retired `uscreen-gui`).
* Every function now has a doc comment (trait-impl methods inherit the trait's), every file has a purpose header (Rust `//!`,
  Kotlin `/* */`, extension, scripts that lacked one). `docs/development.md` layout rewritten.

### 1/4. The desktop layer (`host/src/desktop/`)

`DesktopBackend` trait + `Capabilities { focus, context, placement }`; backends `gnome` (extension file, probe, spawn-at), `x11`
(`_NET_ACTIVE_WINDOW`), `generic` (reads `focus.json`, claims nothing). The focus watcher now polls the backend. `docs/desktop-backends.md`
has the checklist for adding a desktop. Live check after the refactor: log "Desktop backend: gnome", art route, Chrome drags, taps, resting
pen, shaky taps all unchanged.

### 2/3. The pen menu

* **Trigger** (`ghost/auto.rs`): mouse route, tip down for >= 0.6 s without leaving the click-slop zone -> on lift, ask the runner.
  Any movement out of the zone (a drag, a pause-then-drag) never triggers it; art route never triggers it.
* **Context** (extension probe, answered in `context.json`): surface under the pointer, window frame, selection and clipboard *lengths*.
* **Decision** (`menu/decision.rs`, pure): only over an app window and not in its top 48 px (title bar); text highlighted -> Cut, Copy, Select all
  (+ Paste if the clipboard has something); only a clipboard -> Paste, Select all; nothing -> no menu; Undo whenever it shows.
* **Window** (`menu/`, crate `uscreen-menu`, GTK 4): a dark rounded pill, text labels or icons (`pen_menu_style`), closes on Escape, focus loss
  or 8 s, writes `{"action":"copy"}` to a result file. Started by `spawn-at spawn --class app.uscreen.menu --anchor cursor ...`.
  The class is required: without it spawn-at left the window at the compositor's default spot (measured), with it it is placed (~0.18 s).
* **Runner** (`menu/runner.rs`): remembers the focused window, shows the menu, waits for the result, gives focus back (`spawn-at focus --id`, 19 ms),
  sends Ctrl+C/X/V/A/Z through the virtual keyboard.
* **Settings** (config.toml, global): `pen_menu = true` (opt-out), `pen_menu_style = "text"|"icons"`. Not yet in the GUI or the tablet app.
* **Install**: `build-all.sh` builds and deploys `uscreen-menu`; `install.sh` copies it, installs the GTK 4 runtime best-effort, installs/updates spawn-at.


## Result

### Results (GNOME 46 Wayland, SM-X616B over USB adb)

| Test | Result |
|---|---|
| Host tests | 188 passed (long press, decision, actions, spawn-at/context parsing, runner parsing) |
| Long press 0.9 s with text selected and clipboard full, extension answer simulated | menu window at the cursor (440x92), ~0.3 s after the lift; a choice ("copy") closed it and Ctrl+C arrived at the keyboard device (libinput) |
| Nothing selected and empty clipboard / pointer on the shell UI / on the desktop | no window, no keys |
| Menu look | dark rounded pill with five text buttons, seen in a crop of the menu's own area |

### Also in this pass: removed the retired egui GUI (`gui-legacy/`), `scratch/`, `upstream_prep/`, `scripts/deploy_fixes.sh`; install, Makefile, release script, packaging and CI

now name `uscreen-pro-gui` and `uscreen-menu`. The release/packaging/CI paths were already stale (they built a `gui/` folder that no longer existed) and the migrated
steps (Tauri build in the Debian 12 container, PKGBUILD, spec) have **not been run**.

### Follow-up: the menu now appears while the pen is held (and no longer clicks the selection)

Reported: the menu only appeared after lifting (even after 30 s), and by then the lift had clicked the selected text and collapsed it.
Cause: the menu was requested on the lift, and the left-button press went out at pen-down, so the lift was a click on the selection.

Change (`ghost/auto.rs`, `menu/runner.rs`):
* On the mouse route the left-button **press is held back** (`press_deferred`). It goes out: when the pen leaves the click slop (a drag; at the spot the tip landed),
  at the lift (a tap: press and release together), or when a long press is declined.
* A background tick (50 ms) notices a tip held still for 0.8 s and asks the runner (`Hold::Pending`). The runner decides and answers: **menu shown** (`Hold::Shown`: the rest
  of the touch is swallowed, no click, no drag; the selection is never touched) or **declined** (nothing to offer, setting off, art app, no runner: the held-back press is delivered
  and the hold carries on as an ordinary click/drag). A lift before the answer is clicked late only if declined.

Results (Chrome probe, 12 characters selected, pen held 3 s on the selection): menu window up **1.05 s after the press, while the pen was still down**; the page saw no press
(`pd=0 cl=0`) and `sel=12` afterwards. Regression unchanged: 12/12 drag-selects, 3 quick taps (`pd=3`), art-route stroke with pressure, focus flip mid-drag, resting pen silent 5.5 s,
shaky taps 0/4 drags, and the full menu path with the real extension (Copy -> Ctrl+C). 192 host tests.
Not verified: a real tap on a menu button with the pen; the delay to the menu is the 0.8 s hold plus ~0.2 s to place the window (the 0.8 s is `StillCfg::long_press`, not a setting yet).

### Follow-up: no dock icon, no shadow rectangle, hold time setting

Reported after trying it: the menu showed up in the dock (a generic gear icon) and a rectangular edge/shadow showed around the pill.
* **Dock icon**: GNOME lists every running window's app. A hidden desktop entry (`NoDisplay=true`) did not remove it (tried, reverted). Wayland has no skip-taskbar hint. So on GNOME the
  **extension now draws the menu itself** (St widgets as Shell chrome, `stylesheet.css`): no window, nothing in the dock, no focus change, no process to start. New trait capability `menu`
  (`show_menu` / `wait_menu_choice`); the runner uses it first and falls back to the `uscreen-menu` window when the desktop has no `menu` or does not confirm within 400 ms (an older extension).
* **Shadow rectangle** (window fallback): the pill's blurred shadow was clipped by the transparent window margin. The GTK pill now has no blur shadow, a 1 px border and a 2 px margin; the Shell-drawn
  menu uses a normal Shell shadow.
* **Hold time**: `pen_menu_hold_ms` (default 800, clamp 300..3000) is live (read by the ticker like the other options). `pen_menu = false` restores plain immediate clicks (the press is no longer held back).
  The desktop app has a "Pen menu" switch, "Hold time for the menu" and "Menu buttons" (text / icons) under Pen and drawing tablets.

Verified: host tests (192); the window fallback with the old extension (menu up 1.5 s after the press: 0.8 s hold + 0.4 s waiting for the missing menu code + 0.2 s placement); the Shell path with a stand-in for the extension's
menu code (request written with items/style/timeout, ack, result, Ctrl+V sent, no window). **Not verified: the extension's menu code in the real Shell** (extension v5, needs the files copied and a re-login), its look, and the
settings controls in the desktop app.

### Verified in the real Shell (extension v5, after the re-login)

Chrome probe page, text selected, pen held on it: the **Shell drew the menu 1.04 s after the press** (no `app.uscreen.menu` window, nothing for the dock), above the pointer, a dark rounded pill with a soft Shell
shadow and no rectangular edge (seen in a crop of its own area). A real pen tap on its **Copy** button wrote `{"id":...,"action":"copy"}` and Ctrl+C reached the keyboard device. Left alone, it dismissed itself after 8 s
(`action: none`).
Bug found on the way: the request id was nanoseconds (~1.8e18), above JavaScript's exact-integer range (2^53), so the extension echoed a different number; ids are now milliseconds.
Also: after the re-login the tablet app was not connected to the restarted daemon (no input client); `am force-stop` + start of the app reconnected it.

### Follow-up: "clicking on a selection does nothing / drags" (found: Mouse style, then zero-length clicks)

* First finding: during the reported test the tablet profile was in **Mouse** style (log: `Tablet changed stylus_style to "mouse"`, set from the tablet's "Pen acts as" chips). Mouse style presses at touchdown with no click
  zone, so the drift of a normal tap (measured ~1.4 mm per 100 ms) was sent as movement with the button down: libinput showed press, 20-30 motion events, release on sub-100 ms strokes. Set back to `auto`.
* Second finding, in Auto: the recorder showed clean taps (15 taps drifting 0.0-0.9 mm, all inside the 2 mm zone, delivered as motion-free clicks) that still did not deselect in the reporter's app. The one remaining difference from a
  working click (Mouse style: a gentle click deselects) is timing: since the held-back press, a tap was press+release at the same instant. A real quick click on the reporter's touchpad was held 70-147 ms (median ~105).
* Change: a delivered tap now keeps the left button down for `pen_click_hold_ms` (default 90, 0 = release at once), released by a 10 ms ticker; the cursor does not move while it is held (no micro-drag), and a new touch releases it
  first. Verified with libinput: press-to-release 93-99 ms with 0 motion events in between, for taps whose touch lasted 30-110 ms. Regression unchanged (drags 12/12, quick taps, art route, resting pen, shaky taps, menu, Chrome Wayland tap on a selection).
* Not verified: the reporter's app (Google Docs). If it still fails there, the next knobs are a tiny natural motion during the press and a configurable click-zone size.


## Not verified

* **Extension probe: verified after the re-login** (extension v4). With text selected on a Chrome probe page it reported `selection_len: 12`, `zone: window`, the window frame and
  `app_id: google-chrome.desktop`; a 0.9 s long press then showed the real menu (Cut, Copy, Select all, Undo; no Paste, the clipboard was empty), and choosing Copy sent Ctrl+C.
* A real **tap on a menu button with the pen** (the test wrote the choice file); the buttons are ordinary GTK buttons.
* The menu in Krita/art apps is deliberately off. Terminals need Ctrl+Shift+C/V (not handled). AT-SPI (Tier 2: "is this an editable field?")
  is not built; over blank page space with something on the clipboard the menu still offers Paste.
* The transparent margin around the pill shows a faint lighter rectangle in the capture.
* GNOME 46 / Ubuntu 24.04 only. Release packaging (deb/rpm/PKGBUILD, `build-release.sh`) was not updated for `uscreen-menu`.

## Sources

- [`experiments_and_findings/102_pen_menu_and_desktop_layer.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/102_pen_menu_and_desktop_layer.md)
