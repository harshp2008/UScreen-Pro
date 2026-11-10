<!--
  docs/codebase-changes.md: what the cleanup branch (chore/cleanup-and-docs) changed and why.
  Owns: the old-path to new-path table, removals, splits, and what was kept on purpose.
  Does not own: release notes (CHANGELOG.md) or how the code works (docs/architecture.md).
-->

# Codebase changes (cleanup and documentation branch)

Behaviour does not change in this branch. Every phase ends with the same checks:
`cargo test` in `host/` and in `uscreen-pro-gui/src-tauri`, `scripts/tests/run.sh`,
`npx tsc --noEmit`, `./gradlew assembleDebug testDebugUnitTest`, `node --check` on the
extension and `bash -n` on every shell script.

## Verification counts

| Check | Before | Now |
|---|---|---|
| `cargo test` in `host/` | 199 passed, 2 ignored | 199 passed, 2 ignored |
| `cargo test` in `uscreen-pro-gui/src-tauri` | 10 passed | 10 passed |
| `scripts/tests/run.sh` | 53 passed | 60 passed (7 new tests) |
| `npx tsc --noEmit` | clean | clean |
| `./gradlew assembleDebug testDebugUnitTest` | passes | passes |
| `node --check extensions/uscreen-app-lock/extension.js` | ok | ok |
| `bash -n` on every `*.sh` | ok | ok |
| `scripts/tests/in-containers.sh` | not run | 60 passed in each of Ubuntu 24.04, Ubuntu 22.04, Debian 12 and Arch (Fedora and openSUSE images were not available locally and were not pulled) |
| `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo doc` with warnings as errors | clean | clean |
| `cd website && npm run build` (includes the site check) | n/a | 159 pages, check ok |

## Removed (phase 2)

Found with `cargo build` and `cargo clippy --all-targets` (host, Tauri crate and menu: both clean),
`tsc --noUnusedLocals --noUnusedParameters` (clean), a reference count of every exported TypeScript
name, every Kotlin function and constant, every Cargo dependency, every config field of the host
(all are read somewhere), and every shell function. Confirmed by `grep` before each removal.

| What | Where | Why it was dead |
|---|---|---|
| `DEFAULT_QUALITY` | `uscreen-pro-gui/src/lib/options.ts` | exported, referenced nowhere |
| `TouchCapture.sendMode()` and the `pendingMode` replay | `android/.../TouchCapture.kt` | no caller; the tablet changes modes through `sendSetting`. The host still accepts the old `mode` message, so older apps keep working |
| `FadeCurve.NAMES` | `android/.../HostLink.kt` | no reader |
| `tokio-util` dependency | `host/Cargo.toml` | no `tokio_util` in any source file; the host builds and all tests pass without it |

## Kept on purpose

| What | Why |
|---|---|
| `#[cfg_attr(not(feature = "inproc-encoder"), allow(dead_code))]` in `host/src/capture/mod.rs` | the two items are used only by the optional in-process encoder |
| the host's handling of the old `mode` message | protocol compatibility with older Android apps |
| `scripts/redact.sh`, `scripts/rca56_guarded_test.sh`, `scripts/enc_latency_probe.py`, `experiments_and_findings/cpu_stress.py`, `experiments_and_findings/simulate_motion.py` | referenced by experiment records or used on purpose by the maintainer |
| `FadeCurve`'s other members, `ReconnectPolicy`, and all serde fields of `config.toml` | read by the app, by tests, or needed for config round-tripping |

## Unused files deleted (by decision)

| File | Why it went |
|---|---|
| `host/evdi/evdi_capture_test.c`, `evdi_grab_test.c`, `evdi_test2.c` | EVDI experiment programs; in no build, workflow or document |
| `android/.../ui/MorphButton.kt` and the `androidx.graphics:graphics-shapes` dependency | no screen used the button; the dependency was used only there |
| `scripts/test-gen-edid.py` | run by no CI job or Makefile target |
| `uscreen-pro-gui/README.md` | the unedited Tauri template text |

`experiments_and_findings/` was not touched. The old GitHub Pages files in `docs/` stay for now: `scripts/update-release-metadata.sh`
(and `make release-metadata`) rewrite their version and dates, so deleting them means changing the release script first.

## Splits and kept files (phase 3)

Rule: split where a file holds several responsibilities; keep a cohesive unit (one state machine whose
parts share private state) even when it is long. Every split was compiled, linted and tested; nothing
was moved without the build and tests staying green.

| File (lines before) | Decision | Why |
|---|---|---|
| `host/evdi/evdi_helper.c` (1904) | **split** into `evdi_helper.c` (device discovery, `main`) and `helper_state.h`, `helper_signals.h`, `helper_boundary.h`, `helper_timing.h`, `helper_convert.h`, `helper_writer.h`, `helper_events.h` | seven responsibilities (shared state, signals, X11 boundary sync, timing ring, NV12 conversion, frame writer, EVDI events). The parts are `#include`d by `evdi_helper.c`, so the helper is still one translation unit: the hot conversion loops still inline and no `static` becomes global. The preprocessed output of the new tree equals the old one line for line (`gcc -E -P`, blank lines removed, sorted). The Makefile and `scripts/build-release.sh` compile the same file with the same flags, so they need no change beyond a comment |
| `android/.../ui/ConnectionScreen.kt` (715) | **split** into `ConnectionScreen.kt` (`UScreenMain`), `ActiveScreens.kt` (graphics-tablet and touchpad screens), `HostOverlays.kt` (unpaired banner, host notice) | three unrelated groups of composables |
| `android/.../ui/SettingsSheet.kt` (668) | **split**: building blocks to `SettingsParts.kt` | layout helpers (section, switch row, chip choice, curve labels) are separate from the settings themselves |
| `android/.../TouchCapture.kt` (1350) | **kept** (only `HostNotice` moved to `HostLink.kt`, next to `HostConfig`) | one class: the socket, the pen path, the touch path and the touchpad feed share the WebSocket, the sequence counter and the display geometry. Splitting it needs a shared-state object and cannot be checked without a tablet |
| `android/.../VideoReceiver.kt` (865) | **kept** | one decoder pipeline: socket loop, codec tiers and the arrival ring share state |
| `android/.../MainActivity.kt` (802) | **kept** | an Activity whose lifecycle callbacks share the connection objects; no part stands alone |
| `uscreen-pro-gui/src/components/AddDeviceWizard.tsx` (1275) | **kept** | one multi-step dialog with 34 pieces of state used across its steps (a single state machine). Pulling the steps apart would mean passing about 40 props to each and cannot be checked visually here |
| `host/src/daemon/session/monitor.rs` (716) | **kept** | one supervising loop (`adb_monitor`) with local state; no independent parts |
| `host/src/config/mod.rs` (685) | **split**: `FileConfig::sanitize` moved to `config/sanitize.rs` | validation is separate from the shape of the config; the model, defaults and load/save stay together; tests stay next to the model |
| `uscreen-pro-gui/src-tauri/src/adb.rs` (682) | **split** into `adb/` (`mod.rs` types and the process wrapper, `devices.rs`, `install.rs`, `intents.rs`) | listing, installing and device intents are separate; every public name is re-exported, so callers and the Tauri command registration are unchanged |
| `host/src/net/adb.rs` (637) | **split** into `net/adb/` (`mod.rs` types and serial classification, `discovery.rs`, `forwarding.rs`, `wifi.rs`, `app.rs`) | discovery, tunnels, wireless setup and app supervision are separate; every public name is re-exported |
| `host/src/input/uinput/devices.rs` (597) | **kept** | one aggregate, `InjectDevices`, whose methods work on the same device fields |
| `host/src/input/ghost/auto.rs` (470) | **kept** | the Dynamic pen state machine; its states share private fields |
| `extensions/uscreen-app-lock/extension.js` (627) | **split** into `extension.js` (enable/disable), `lib/*.js` and `compat/*.js` | four independent jobs (app lock, pen routing, context probe, pen menu) and the Shell-version code |

The test in `host/src/net/adb/mod.rs` named `physical_device_deduplication_prefers_usb` re-implements the
merge inline instead of calling production code. It is left as it is; a real test of `scan_adb_devices`
needs a fake `adb`.

## Where things moved (phase 5)

| Old path | New path |
|---|---|
| `host/src/desktop/generic.rs` | `host/src/desktop/generic/mod.rs` |
| `host/src/desktop/x11.rs` | `host/src/desktop/x11/mod.rs` |
| `host/src/display/x11.rs` | `host/src/desktop/x11/display.rs` |
| `host/src/desktop/gnome.rs` | `host/src/desktop/gnome/mod.rs` |
| `host/src/display/gnome.rs` | `host/src/desktop/gnome/display.rs` |
| `host/src/desktop/spawn_at.rs` | `host/src/desktop/gnome/spawn_at.rs` |
| `host/src/display/kwin.rs` | `host/src/desktop/kde/display.rs` |
| `host/src/daemon/osk.rs` | `host/src/desktop/kde/osk.rs` |
| `host/src/display/hyprland.rs` | `host/src/desktop/hyprland/display.rs` |
| `host/src/display/cosmic.rs` | `host/src/desktop/cosmic/display.rs` |
| `host/src/config/mod.rs` (`FileConfig::sanitize`) | `host/src/config/sanitize.rs` |
| `host/src/net/adb.rs` | `host/src/net/adb/` (`mod.rs`, `discovery.rs`, `forwarding.rs`, `wifi.rs`, `app.rs`) |
| `uscreen-pro-gui/src-tauri/src/adb.rs` | `uscreen-pro-gui/src-tauri/src/adb/` (`mod.rs`, `devices.rs`, `install.rs`, `intents.rs`) |
| `host/evdi/evdi_helper.c` (parts of it) | `host/evdi/helper_state.h`, `helper_signals.h`, `helper_boundary.h`, `helper_timing.h`, `helper_convert.h`, `helper_writer.h`, `helper_events.h` (`evdi_helper.c` stays the file that is compiled) |
| `android/.../ui/ConnectionScreen.kt` (parts) | `ui/ActiveScreens.kt`, `ui/HostOverlays.kt` |
| `android/.../ui/SettingsSheet.kt` (parts) | `ui/SettingsParts.kt` |
| `HostNotice` in `android/.../TouchCapture.kt` | `android/.../HostLink.kt` |
| `extensions/uscreen-app-lock/extension.js` (all but enable/disable) | `lib/appLock.js`, `lib/penRouting.js`, `lib/contextProbe.js`, `lib/penMenu.js`, `lib/dataFiles.js`, `compat/monitors.js` |
| `scripts/lib/platform.sh` (per-family `case` blocks) | `scripts/lib/distros/debian.sh`, `fedora.sh`, `arch.sh`, `suse.sh`, `unknown.sh` |
| `scripts/lib/evdi.sh` (Debian, Arch, openSUSE methods and the dpkg repair) | the same family files |
| `scripts/install.sh` (`install_deps` and the GTK runtime `case`) | the same family files |

Rust module paths changed with the files: `crate::display::{gnome,x11,kwin,hyprland,cosmic}` is now
`crate::desktop::{gnome,x11,kde,hyprland,cosmic}::display`, and `daemon::osk` is `desktop::kde::osk`. Nothing
outside the host crate refers to them. No public name changed: the CLI, `config.toml`, the protocol, the files the
installer and CI read, and the Tauri command names are as before.

## Added in phase 5

| What | Why |
|---|---|
| `extensions/uscreen-app-lock/compat/` (`index.js`, `versions.js`, `monitors.js`) and `tests/check-versions.mjs` | one place for per-Shell-version code, one table of Shell versions, and a check that the table agrees with `metadata.json` |
| extension `version` 8 → 9 in `metadata.json` | the extension's files changed, and the installer uses the number to decide that a log out is needed once |
| the extension's first log line now includes the Shell version and what is known about it | the only new runtime output |
| `scripts/lib/distros/_template.sh` and six installer tests (54 → 60) | the interface every family implements, and tests for it |
| `docs/adding-support.md` | the guide to adding a distribution, a desktop or a GNOME version |
| CI: an `extension` job (syntax of every module, version lists), ShellCheck over `distros/`, and checks that the installed extension has its modules | the same checks the local gates run |

### Fixes found while moving files

- The release tarballs (`scripts/build-release.sh`, `make dist-local`) copied `install.sh` but not `scripts/lib/`,
  which `install.sh` sources, and not the extension. They now copy `scripts/lib/` and `extensions/uscreen-app-lock/`
  (without its `tests/`). Before this, a tarball install could not start.
- `make install` copied only `extension.js`, `metadata.json` and the stylesheet; it now also copies `lib/` and `compat/`.
- `scripts/install.sh` deletes the extension's old `lib/*.js` and `compat/*.js` before copying the new ones, so a module
  removed later does not linger on an upgrade.

## The documentation site and the findings (phases 7 and 8)

- `website/` is a Starlight (Astro) site built like the spawn-at project's: the same Starlight and Astro versions, the same
  theme CSS, the `records` and `code-breaks` integrations, the table of contents toggle, and a GitHub Pages workflow
  (`.github/workflows/docs.yml`). The framework was confirmed from that project's `package.json` and `astro.config.mjs`.
  Where it differs: the sidebar, the content, and Node scripts instead of `cargo xtask` for generating and checking pages.
- Generated at build time (not committed): the Project documents (from `docs/`), the release notes (from `CHANGELOG.md`), the
  CLI reference (from `uscreen --help`) and the configuration page (from the config structs).
- Committed: the hand-written guide pages and the 62 experiment records converted from `experiments_and_findings/`.
- The site is built with `cd website && npm ci && npm run build`; the build ends with a check that fails on a broken link or
  anchor, a request to another site, an en dash where a flag may have been, or personal data.
- `uscreen doctor`, `uscreen --help` and the rest were not changed. The rustdoc build now passes with warnings as errors (three
  documentation comments were corrected for that).

### What was redacted before publishing

`website/scripts/redact.mjs` runs over every generated or converted page: home paths (72 in the experiment records),
the maintainer's login and host name written as `harsh` in journal and process output (160), e-mail addresses (15, the
extension uuid and git's remote user excepted), IP addresses (1), a MAC address (1) and device serials (3). The raw
write-ups in `experiments_and_findings/` are not edited, and their screenshots and logs are not published.

## Comments (phase 1)

- The comment rules are in [comment-style.md](comment-style.md).
- Fixed while commenting (comment contradicted the code):
  `uscreen-pro-gui/src/App.tsx` said the settings file is re-read "twice a second-ish"; the code polls every 1.5 s.
  The extension header said "Three jobs" and listed four.
- Left for later steps of the cleanup: the C helper (commented while it is split) and the installer
  scripts (commented after they move).

## Known leftovers, not changed

- `./gradlew lintDebug` reports four `SuspiciousIndentation` errors in `TouchCapture.kt`
  (lines with `if (...) continue` followed by statements) that exist in the original code.
  They are style warnings from the linter, not defects, and lint is not part of the build.
- `host/src/daemon/update.rs` checks the upstream project's releases
  (`majmichu1/UScreen`), not `harshp2008/UScreen-Pro`. That is behaviour, so it is not changed here.
- `scripts/e2e_input_engine.sh` has a default device serial in it (`SERIAL=${SERIAL:-...}`).
  It is a test script for the maintainer's tablet; the website redaction pass covers published text only.
