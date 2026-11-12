<!--
  docs/cleanup/CHANGES.md: what the chore/codebase-cleanup branch moved, removed, split and left alone, with the check results before and after.
  Owns that record. The earlier cleanup is in ../codebase-changes.md; the evidence for removals is in removed.md; the follow-ups are in post-review.md.
-->

# Changes of the codebase-cleanup branch

Behaviour is meant to be unchanged: no feature, default, config key, wire message, Tauri command or file name the apps
read was changed. The one theoretical difference is listed under "Display dispatcher" below.

## Checks, before and after

| Check | Before (`ed97427`) | After |
|---|---|---|
| `cargo test --workspace` | 217 passed + 2 ignored (host), 11 (Tauri crate) | 217 passed + 2 ignored, 11 |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass | pass |
| `cargo fmt --check` | **fail** (5 hunks in the Tauri crate) | pass |
| `npx tsc --noEmit` | pass | pass, also with `--noUnusedLocals --noUnusedParameters`; `vite build` succeeds |
| GUI lint and tests | none exist | none exist |
| `./gradlew testDebugUnitTest` | pass, 44 tests | pass, 47 tests (3 new, `PenTiltTest`) |
| `scripts/tests/settings_parity.py` | pass (35 desktop keys counted) | pass (33; it now reads `power/shared.ts` and no longer counts two props in `PowerSettings.tsx`) |
| `scripts/tests/run.sh` (installer) | 60 passed | 60 passed |
| `check-versions.mjs` (extension) | ok | ok |
| `./build-all.sh` | not run at baseline | succeeds; installed binaries and APK equal `dist/` |

Lines: 161 files changed, +4,110 / -2,947 against the baseline. Counting only code and scripts: +3,227 / -2,846, almost all of
it moved (six test modules, the KDE helpers, the Android packages' import lines) or added comments. Net removed code is small
because little was unused (removed.md).

## What moved (old path to new path)

| Old | New |
|---|---|
| `docs/architecture.md` | `docs/architecture/overview.md` (rewritten) |
| `uscreen-pro-gui/src/components/AddDeviceWizard.tsx`, `ManualPairDialog.tsx` | `components/wizard/` |
| `uscreen-pro-gui/src/components/CurveEditor.tsx` | `components/power/CurveEditor.tsx` |
| `uscreen-pro-gui/src/components/Sidebar.tsx`, `TitleBar.tsx` | `components/shell/` |
| `android/.../TouchCapture.kt`, `TouchpadEngine.kt`, `TapToClick.kt`, `TapGate.kt`, `NativeTapDetector.kt`, `PalmFilter.kt`, `TouchpadShape.kt`, `Haptics.kt`, `ClickSound.kt` | `android/.../input/` (package `com.uscreen.input`) |
| `android/.../VideoReceiver.kt` | `android/.../video/` (package `com.uscreen.video`) |
| the Android unit tests of those classes | `android/app/src/test/.../input/` |
| the KWin and kscreen-doctor helpers in `host/src/input/mapping.rs` (`primary_non_evdi_output`, `kwin_device_property`, `output_area_on_desktop` as `output_area`, `kscreen_outputs`, `target_output`) | `host/src/desktop/kde/display.rs` |
| the KDE branches of `enable_evdi_display` and `disable_evdi_display` in `host/src/capture/display.rs` | `desktop/kde/display.rs` (`enable_output`, `disable_output`) |
| the KWin D-Bus layer of `desktop/kde/display.rs` | `desktop/kde/kwin.rs` (re-exported from `display.rs`) |

Entry points named in the Android manifest and the ADB intents the host sends (`MainActivity`, `StreamingService`,
`IdentifyReceiver`, `RemoveAdminReceiver`, `UScreenAdminReceiver`) stayed in `com.uscreen`. The ProGuard rule for
`TouchCapture$*` now names `com.uscreen.input.TouchCapture$*`.

## What was added

| What | Where |
|---|---|
| the display dispatcher (`DisplayBackend`, `backend()`, `enable_output`, `disable_output`, `connector_names`) | `host/src/desktop/display.rs` |
| `desktop::command_exists` | `host/src/desktop/mod.rs` |
| `PenTilt` and three unit tests | `android/.../input/PenTilt.kt`, `PenTiltTest.kt` |
| `lib/scroll.ts`, `lib/configMerge.ts`, `components/wizard/model.ts`, `components/power/shared.ts` | extracted, see below |
| 134 `PLATFORM(<name>)` tags | `git grep -n "PLATFORM("` |
| `cargo fmt --check` and the parity script in CI | `.github/workflows/build.yml` |
| docs | `docs/architecture/platform-modularity-plan.md`, `platform-modularity.md`, `docs/cleanup/{baseline,removed,post-review,CHANGES}.md`, `experiments_and_findings/106_codebase_cleanup.md` |

## What was removed

Three items and three merged helpers, each with its evidence in [removed.md](removed.md): a stale
`#[allow(unused_labels)]`, the unused Compose colour `SurfaceHigh`, three copies of `errText`, five copies of the PATH
lookup and four copies of the connector filter.

## What was split

| File | Result | Why it is a real seam |
|---|---|---|
| `App.tsx` (641 lines) | `lib/scroll.ts`, `lib/configMerge.ts` (pure functions) | logic with no React in it |
| `PowerSettings.tsx` (544) | `power/shared.ts`, `power/ModePanel.tsx`, `power/TabletPanel.tsx`; `PowerSettings.tsx` re-exports | two independent panels and their shared types |
| `AddDeviceWizard.tsx` (1,301) | `wizard/model.ts` (step list and state types) | types and constants only; the component is not split (see below) |
| `TouchCapture.kt` | `PenTilt.kt` | the one stateless piece |
| six Rust files (`config/options/mod.rs`, `daemon/guard/mod.rs`, `capture/packetizer.rs`, `input/physical.rs`, `config/mod.rs`, `desktop/x11/display.rs`) | each loses its inline test module to a sibling `tests.rs` | tests versus code; 1,300 lines moved as is |
| `desktop/kde/display.rs` (683) | `kde/kwin.rs` (D-Bus) and `display.rs` (kscreen-doctor) | two tools, two layers |

## Display dispatcher (the one reorganisation that is more than a move)

`input/mapping.rs`, `capture/display.rs`, `cli/doctor/display.rs`, `cli/doctor/system.rs` each had an `if` ladder over the
desktops. They now `match` on `desktop::display::backend()`, which returns Hyprland, COSMIC, GNOME, X11 or KDE (the
fall-through, as before). The order is one: Hyprland, COSMIC, GNOME, X11, KDE. The old input-mapping code tested X11 before
COSMIC where the enabling code tested COSMIC first; COSMIC has no X11 session, so on a real system the two orders agree. This is the
only place where a combination of environment values that cannot occur could now give a different answer.

## What was left alone, and why

| Item | Why |
|---|---|
| `TouchCapture.kt`, `VideoReceiver.kt`, `MainActivity.kt`, `AddDeviceWizard.tsx` (the component), `adb_monitor` | each is one state machine with shared state; splitting needs a tablet or a device to verify. See post-review.md A to D |
| input injection, capture source, service manager, paths | one implementation each: the seam is documented, no trait invented |
| `scale::backend()` | decides the desktop its own way; merging would change an unknown-desktop case |
| `desktop::detect()` | a separate decision (focus and menu) with its own order |
| `inproc-encoder`, `Makefile VERSION`, the upstream repository in `update.rs`, the `mode` message | behaviour or release tooling; see removed.md |
| the website, `experiments_and_findings/` (except the new record) | outside the cleanup |

## Needs real hardware

* The tablet app after the package move: touch, pen (including tilt), touchpad, video. The tablet running here was updated to
  the new build by the daemon and connected, but nobody drove it.
* A second device (the phone was offline) and multi-tablet sessions.
* Hyprland, COSMIC and KDE Plasma: the dispatcher's arms for them are the old code moved or called differently. The cleanup ran
  on Ubuntu GNOME 46 Wayland, where the daemon built from this branch started, chose the GNOME backend and logged "Mapped pen
  and touch to DVI-I-1" with the tablet attached. The KDE, Hyprland and COSMIC paths are covered by the unit tests of their
  parsers, not by a run.
* Desktop app UI after the component moves: `vite build` and `tsc` pass, but nobody opened the wizard or the settings pages.
