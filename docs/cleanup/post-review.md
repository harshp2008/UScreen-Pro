<!--
  docs/cleanup/post-review.md: the review of the tree after steps 1 to 5 of the cleanup, ordered by value and risk.
  Owns the findings and what became of each. What changed is in CHANGES.md.
-->

# Post-cleanup review

Read as a fresh reviewer, after the comments, removals, splits, platform tags and the display dispatcher. Each finding says
whether it was applied (step 7) or left, and why.

## Applied

| # | Finding | Value | Risk | What was done |
|---|---|---|---|---|
| 1 | The same `$PATH` lookup (`command_exists`) was written five times (GNOME, X11, COSMIC, the doctor, the scale detector) | medium: the doctor and `active()` could disagree on what "installed" means | none | one `desktop::command_exists` |
| 2 | The magic numbers `0..20` and `250 ms` in three input-mapping retry loops, while the warnings say "within 5s" | low | none | named constants `MAP_ATTEMPTS`, `MAP_RETRY_DELAY` in `input/mapping.rs` |
| 3 | CI ran no format check and no settings-parity check, which is how `cargo fmt` drifted in the Tauri crate | medium | low | two steps in `.github/workflows/build.yml` (not run on GitHub from here; both pass locally) |
| 4 | `desktop/kde/display.rs` mixed the KWin D-Bus layer with the kscreen-doctor output code and had grown to 683 lines | medium | none | D-Bus layer and its tests to `kde/kwin.rs`; paths kept by re-export |
| 5 | A doc comment in `kde/display.rs` had come loose from the test module it described (found while splitting) | low | none | moved back |

## Left as follow-ups

Ordered by value. "Needs hardware" means the change cannot be checked by the compiler, the unit tests and the installer tests.

| # | Finding | Value | Risk | Why it was left |
|---|---|---|---|---|
| A | **No test for `lib/configMerge.ts`**, the three-way merge that keeps tablet-side edits and local edits apart. It is real logic with a history of bugs and the desktop app has no test runner at all (`package.json` has `dev`, `build`, `preview`, `tauri`) | high | low | adding Vitest is new tooling in the build; the project owner should decide |
| B | **`AddDeviceWizard.tsx` is one 1,290-line component** with about 34 pieces of state; its five steps could be separate components fed by a reducer | high | medium | cannot be checked without clicking through the wizard with a device; prop plumbing for about 40 values |
| C | **`TouchCapture.kt` (1,400 lines)** holds the socket, the pen path, the touch path and the touchpad feed over one WebSocket and one sequence counter; `VideoReceiver.kt` (870) and `MainActivity.kt` (840) are similar. Natural collaborators: `PenPath`, `TouchpadFeed`, `HostSocket` | high | high | needs the tablet to verify pen, touch and touchpad behaviour. Only the stateless pen tilt maths was extracted |
| D | **`adb_monitor` in `daemon/session/monitor.rs` is a 700-line function** | medium | high | needs two devices and the adb event sequences to verify |
| E | **Three detectors for "which desktop"**: `desktop::detect()` (focus and menu), `desktop::display::backend()` (output and input mapping) and `desktop::scale::backend()`, which recognises KDE by `kscreen-doctor` being on `PATH` while the other treats KDE as the fall-through | medium | medium | merging changes what an unknown desktop with `kscreen-doctor` installed does |
| F | **`output_of` exists in four variants** (`kde`, `gnome`, the doctor, and `x11::run`) that differ in whether failure is an error, whether output is trimmed, and whether stderr is joined | medium | medium | a shared helper needs an explicit variant per caller; each has its own tests only through its parser |
| G | **`$HOME` is read in twelve places with different fallbacks** (empty string, `/tmp`, `None`), and `~/.config/uscreen` / `~/.local/share/uscreen` are spelled out in Rust, shell and Kotlin-free scripts | medium | medium | a `paths` module per crate; needed before any non-XDG platform |
| H | **Settings keys are listed in five places**: `config/options/device.rs`, `TabletProfile`, `HostConfig.kt`, `PowerValues`, and the `HEAVY` list in `lib/configMerge.ts`. `settings_parity.py` checks three | medium | low | extending the script to check that every `HEAVY` key is a real config key is a small change, but it is new test behaviour |
| I | **No `ServiceManager` seam**: systemd is called directly from the desktop app, the tray, `build-all.sh`, the Makefile and the installer | medium | medium | one implementation exists; the plan documents the seam (group 9) |
| J | `kwin_device_property` in `kde/display.rs` is a D-Bus read and belongs in `kde/kwin.rs` | low | none | it is called by `input/mapping.rs` by its current path; moving it is a rename of one call |
| K | The KDE input-mapping tail (`map_devices_to_output`) is still in `input/mapping.rs` because it needs `DeviceIdentity` from `input` | low | low | moving it makes `desktop` depend on `input` types, the wrong direction |
| L | `DEBUG_INPUT = true` in `TouchCapture.kt` | low | none | it is a constant that is always on; the logging it gates is cheap, but a build-type switch would be cleaner. Not changed because the maintainer uses `adb logcat -s UScreenInput` |
| M | `Makefile` `VERSION = 1.2.12` against crates at `1.3.0` | low | low | release tooling; see `removed.md` |
| N | `inproc-encoder` is never built by CI or `build-all.sh` | low | low | an option of the project; see `removed.md` |
| O | The Android `uses-permission` and component list is one `PLATFORM(android)` tag; nothing checks it against use | low | none | tooling decision |
| P | No automated test that the display dispatcher picks the right backend; `backend()` reads the environment and `PATH` | low | none | needs an injectable environment |

## Checked and found fine

* Error handling: the 46 non-test `.unwrap()` calls counted by a first search were nearly all inside inline test modules
  (`avi.rs`, `scale.rs`); no production `unwrap` was found on a path that handles external input.
* Logging: `println!` outside `cli/` is only command output (`main.rs`, the control socket reply), not diagnostics.
* No file mixes a protocol type with its handler; `input/protocol/` holds only wire types.
