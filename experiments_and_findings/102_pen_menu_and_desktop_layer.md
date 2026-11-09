# 102: Codebase cleanup, the desktop layer, and the pen's long-press menu

Three pieces of work in one pass: (0) cleanup of the host, (1/4) a desktop layer so features do not hardcode GNOME,
(2/3) the long-press menu (cut / copy / paste / select all / undo) built on it.

## 0. Cleanup (no behaviour change; 169 tests and the live checks were identical before and after)
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

## 1/4. The desktop layer (`host/src/desktop/`)
`DesktopBackend` trait + `Capabilities { focus, context, placement }`; backends `gnome` (extension file, probe, spawn-at), `x11`
(`_NET_ACTIVE_WINDOW`), `generic` (reads `focus.json`, claims nothing). The focus watcher now polls the backend. `docs/desktop-backends.md`
has the checklist for adding a desktop. Live check after the refactor: log "Desktop backend: gnome", art route, Chrome drags, taps, resting
pen, shaky taps all unchanged.

## 2/3. The pen menu
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

## Results (GNOME 46 Wayland, SM-X616B over USB adb)
| Test | Result |
|---|---|
| Host tests | 188 passed (long press, decision, actions, spawn-at/context parsing, runner parsing) |
| Long press 0.9 s with text selected and clipboard full, extension answer simulated | menu window at the cursor (440x92), ~0.3 s after the lift; a choice ("copy") closed it and Ctrl+C arrived at the keyboard device (libinput) |
| Nothing selected and empty clipboard / pointer on the shell UI / on the desktop | no window, no keys |
| Menu look | dark rounded pill with five text buttons, seen in a crop of the menu's own area |

## Not verified
* **Extension probe: verified after the re-login** (extension v4). With text selected on a Chrome probe page it reported `selection_len: 12`, `zone: window`, the window frame and
  `app_id: google-chrome.desktop`; a 0.9 s long press then showed the real menu (Cut, Copy, Select all, Undo; no Paste, the clipboard was empty), and choosing Copy sent Ctrl+C.
* A real **tap on a menu button with the pen** (the test wrote the choice file); the buttons are ordinary GTK buttons.
* The menu in Krita/art apps is deliberately off. Terminals need Ctrl+Shift+C/V (not handled). AT-SPI (Tier 2: "is this an editable field?")
  is not built; over blank page space with something on the clipboard the menu still offers Paste.
* The transparent margin around the pill shows a faint lighter rectangle in the capture.
* GNOME 46 / Ubuntu 24.04 only. Release packaging (deb/rpm/PKGBUILD, `build-release.sh`) was not updated for `uscreen-menu`.

## Also in this pass: removed the retired egui GUI (`gui-legacy/`), `scratch/`, `upstream_prep/`, `scripts/deploy_fixes.sh`; install, Makefile, release script, packaging and CI
now name `uscreen-pro-gui` and `uscreen-menu`. The release/packaging/CI paths were already stale (they built a `gui/` folder that no longer existed) and the migrated
steps (Tauri build in the Debian 12 container, PKGBUILD, spec) have **not been run**.
