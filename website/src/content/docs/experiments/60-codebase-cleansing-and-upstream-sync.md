---
title: "60 Codebase cleansing and upstream sync"
sidebar:
  order: 42
  label: "60 Codebase cleansing and upstream sync"
---

Date: 2026-11-08. Branches: `refactor/cleansing-v2` (merged into `main`), `feat/upstream-sync`.

| Field | Value |
| :-- | :-- |
| **ID** | 60 |
| **Date** | 2026-11-08 (as written in the file) |
| **Build** | `8bb13b5`, `b306058`, `fae41ab`, `edb5cc5` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Phase 1 — cleansing (`refactor/cleansing-v2`)

| Step | Commit message | What changed |
|---|---|---|
| 1 | `docs: comprehensive codebase commenting…` | Doc comments on every previously undocumented public Rust item (about 120), module headers for the six Rust files that lacked one, KDoc on the Kotlin classes/functions that lacked it (`///` converted to `/** */`), JSDoc on React exports and types. |
| 2 | `refactor: remove dead code, unused imports…` | Removed `#[allow(dead_code)]` suppressions and the code behind them: `adb_device_serial`, `pick_device`, `ensure_edid`, `DeviceProfile`, `perform_secure_pair`, `timing_path`, `InputServer::stop`, `StreamServer::stop`, the unread `hevc_low_latency_tuning` field of `CaptureConfig`, the unread `display_id` wire fields; Kotlin `WS_URL`, `setVirtualResolution`, `obtainMotionEventWithDisplay`, `restartSocket`, `StatusItemCard`, 45 unused imports. Fixed the two clippy errors (`&PathBuf` argument, items after a test module). |
| 3 | `refactor: modularize monoliths…` | `capture.rs` (2448 lines) → `capture/{mod,display,helper,spawn,supervisor,reader,packetizer}.rs`; `input.rs` (2000) → `input/{mod,protocol,uinput,mapping,server,events}.rs`; `main.rs` (834) → `main.rs` + `cli.rs` + `daemon.rs` + `commands.rs`; `MainActivity.kt` platform helpers → `PlatformSupport.kt`; wizard presentational parts → `components/wizard/WizardParts.tsx`. Code moved, not rewritten; only visibility (`pub(super)`) and imports changed. The NAL scan helpers moved from `CaptureManager` associated functions to free functions in `packetizer.rs` to break a circular dependency. |
| 4 | `refactor: organize directory structure…` | `display/` (edid, vdisplay, x11, kwin, gnome, hyprland, cosmic) and `net/` (adb, pairing, installer, stream, notices); sorted `mod` blocks; `cargo fmt`. |
| 5 | `docs: final comment audit…` | Fixed comments that named old file paths, rewrote the module headers that described the monolith, README architecture section. |

Checks: `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p uscreen` 59 passed (same count before and after the split); `tsc --noUnusedLocals --noUnusedParameters` clean; `./gradlew compileDebugKotlin` succeeds.

Not done / limits:

- **Android Lint did not run.** `./gradlew lintDebug --offline` fails on uncached androidTest dependencies; the `lint-results-debug.txt` in `android/app/build/reports` predates this work. Dead Kotlin was found by a symbol-reference scan plus the compiler instead.
- Three Kotlin deprecation warnings remain (`setDecorFitsSystemWindows`, `ACTION_UNINSTALL_PACKAGE`, `rememberPlainTooltipPositionProvider`); replacing them would change behaviour.
- `TouchCapture.kt`, `VideoReceiver.kt` (about 800 lines each) and `doctor.rs`/`session.rs` (about 1200) were not split: their size comes from one stateful class or one long sequential report, and splitting those without changing logic was not worth the risk.
- `AddDeviceWizard.tsx` is still 1229 lines (the stateful component).

### Step 6 audit

`./build-all.sh` succeeded (APK, daemon, Tauri app; deployed to `~/.local/bin`, `uscreen.service` restarted). The first attempt failed with `graph.bin (No such file)` in Gradle's dex step because two build-all runs overlapped; clearing `android/app/build/intermediates/desugar_graph` fixed it.

After deployment, from the daemon journal:

- Daemon `4747887-dirty`, module paths `uscreen::net::stream`, `uscreen::input::server`, `uscreen::capture::supervisor` (the new layout).
- Guard: 12 guard unit tests pass; no safe-mode or trip line at start-up.
- Cursor: `evdi_helper` streaming with `blocked-by-ghost-guard 0`; the boundary-nudge window code is unchanged. I did not move a cursor across the screen boundary and look at the tablet, so the ghost-cursor behaviour itself was not exercised.
- Routing: `Device connected on Slot 1 over USB (DEVICE_SERIAL)`, reverse ports 8890/8891 set up, app launched. The second adb entry (`192.0.2.10:5555`) is the same tablet over Wi-Fi, so **two-tablet routing was not exercised**; only the unit tests cover it.

### Merging into `main`

`main` carried three commits (`8bb13b5`, `3511478`, `b306058`) that this lineage already has as `f968660`, `1663616`, `8577a06` (same messages; the first two have identical diff stats, the third differs slightly). A real merge conflicted in 14 paths because of that duplication and because files had been split. I merged with `-X theirs`, then reset the index to the refactor tip, so the merge commit's tree is exactly `refactor/cleansing-v2` (`git diff HEAD refactor/cleansing-v2` is empty). Whatever main's three commits contained beyond their duplicates here (the old `gui/src/main.rs` egui changes, old `host/src/input.rs`/`pairing.rs`) is superseded, not preserved. Nothing was pushed.

### Phase 2 — upstream sync (`feat/upstream-sync`)

Upstream `majmichu1/UScreen` is 40 commits ahead (up to `fae41ab`, release 1.2.14). Its merge base with this fork is `39ca854`; `evdi_helper.c` differs by about 2500 lines between the two, so nothing was merged — each fix was re-implemented against this tree.

**The "Lenovo compatibility fix" is documentation only.** `edb5cc5` adds two rows to `docs/compatibility.md` (Lenovo Tab4 8 Plus works on Fedora 44 Cinnamon). The code behind that report is the Cinnamon mapping work (`487b33a`), which is ported.

### Ported

| Upstream | Where it landed | Notes |
|---|---|---|
| `5305690`, `354f0f8` per-frame send (AVI) | `capture/avi.rs`, `capture/reader.rs`, `capture/spawn.rs`, `capture/packetizer.rs`, `Codec::name` | ffmpeg writes AVI with `-flush_packets 1`; frames leave when complete instead of after two later frames. Checked that ffmpeg's AVI keeps SPS in-band for H.264. 8 AVI tests + 1 packetizer test. |
| `e9c44f4` VAAPI `async_depth` | `encoders::has_option`, `capture/spawn.rs`, `encoder.rs` | This fork already passed `-async_depth 1` unconditionally; now only when ffmpeg lists the option (ffmpeg before 5.0 rejects it). |
| `35f2415` EDID size bytes | `display/edid.rs`, `scripts/gen-edid.py`, `scripts/test-gen-edid.py` | Bytes 13/14 of the detailed timing were swapped; `EDID_GENERATION` 3 so cached files regenerate. |
| `6788937` keyframe after auth | `net/stream.rs` | Local processes can no longer force keyframes by opening the port. |
| `55e3093` fixed adb reverse ports | `net/adb.rs` (`reverse_pairs`) | **Adapted.** Upstream maps device 8890/8891 to the host ports only. This fork launches the app with port extras, so each tablet's own ports stay mapped and the fixed device ports are added; `adb reverse` is per device, so slots do not collide. |
| `487b33a` Cinnamon + RandR | `display/gnome.rs` (`map_cinnamon_devices`), `display/x11.rs` (`output_changes`, `is_ours`, `belongs_to`), `input/mapping.rs`, `input/server.rs`, `host/Cargo.toml` (`x11rb`) | The per-slot EDID serial part already existed here. Written for X11/Cinnamon; **not run on a Cinnamon session** (this machine is not one). |
| `5466446` decoder failure recovery | `VideoReceiver.kt` | Fatal decoder errors step down the decoder tier and reconnect. Compiles; not exercised with a failing decoder. |
| `17e389f`, `9a0b074` pending-config replay | `TouchCapture.kt` | Accepted settings are no longer re-sent on reconnect. |
| `6b9d36c`, `323fb2e` (host half) | `cli.rs`, `commands.rs`, `config_handler.rs` | `uscreen check-encoder`. The egui half is ignored; the Tauri app does not call it yet. Upstream's `host/tests/check_encoder.rs` (shell-script fake ffmpeg) was not copied. |
| `f7221e0` NVIDIA + VAAPI hint | `encoders.rs`, `doctor.rs` | |
| `42e0d56` KWin 6.8.2 wording | `doctor.rs` | Doc changes in that commit ignored. |
| `edb5cc5` Lenovo rows | `docs/compatibility.md` | |
| `6c09add` (part) | `host/evdi/evdi_helper.c` | Only the FIFO pipe-size step-down. |
| `bc38cd0`, `95be5d3`, `fd873bd`, `aaac97d`, and the `Changelog:` commits `fae41ab`, `186bc8b`, `d785527` | via the commits above and `CHANGELOG.md` | Merges and changelog entries of ported work. |

### Already present here

`70f56e8` (#41): the helper must not grab or re-request while an update is pending. This fork's event loop already never does (see the comment above `g_update_pending` in `evdi_helper.c`), and has no 250 ms watchdog.

### Ignored

| Upstream | Why |
|---|---|
| `4942d73`, `9d02ef9`, `0bc5f8e` bounded conversion threads | This helper already bounds its pool (`nproc / 4`, max 4). |
| `6c09add`, `5ee4d28`, `588a71f`, `9d02ef9` idle-frame comparison, writer idle wait | Would suppress unchanged frames. This fork deliberately flushes a duplicate frame for isolated updates (`3afada8`) and has its own writer pacing and ghost guard; the two designs conflict. |
| `4334042`, `ebc49a0` releases | Version bumps; this fork stamps versions from git. |
| `gui/src/main.rs` changes inside `6b9d36c`, `fae41ab`-era egui work | Legacy egui frontend. |
| `f5bd728`, `ae8f1a0`, `8e0c630`, `c7164be`, `d1836d9` | Upstream's config reference (their schema), website files, wireless design note (this fork has its own pairing), changelog tidy. |

### Touchpad mode (ported afterwards)

`d30d203`, `74a9716`, `0394cb5`, `8fcfe9e` (#40) were first deferred, then ported on request.

- Host: `UScreen Touchpad` relative-mouse uinput device (`input/uinput.rs`, made on first use and never mapped to an output), `rel` / `button` / `scroll` messages (`input/protocol.rs`, `input/events.rs`), held buttons released on disconnect. 3 new host tests.
- Android: `TouchpadGestures.kt` (gesture recogniser, no Android types), hooks in `TouchCapture.handleMotionEvent`, `Prefs.touchpadMode`, a "Touchpad mode" switch in this fork's Compose `SettingsSheet`, applied live. `liftTouches()` now also cancels the recogniser, so disabling touch or blocking input while dragging cannot leave a button held. 14 JVM tests (`TouchpadGesturesTest`, needs `testImplementation junit`).
- Adapted: the switch is disabled while "Touch input" is off, because touch input off already means no finger events.


## Result

### Verification of the sync

`cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test -p uscreen` 73 passed (59 before the sync), `scripts/test-gen-edid.py` passes, `uscreen check-encoder libx264` → ok and an unknown encoder → error, `./gradlew compileDebugKotlin` succeeds.

### Deployed result (build `97a9d80`, clean tree)

`./build-all.sh` finished with exit 0 and replaced `~/.local/bin/uscreen`, `uscreen-pro-gui` and the bundled APK, and restarted `uscreen.service`. The restarted daemon's installer saw the tablet on `4747887-dirty`, queued `adb install -r`, and the tablet now reports `versionName=97a9d80`.

With the AVI path live on the SM-X616B over USB: `Extracted codec config (SPS+PPS): 41 bytes`, about 160 access units per 5 s while the screen was busy, no warnings other than the existing "10-bit was asked for but h264_vaapi is 8-bit" line, reverse ports 8890/8891 in place.

Host latency report, same tablet, before the sync build (restart at 11:44) and after:

| | `encode` stage | host total | TRUE E2E |
|---|---|---|---|
| before (`4747887`) | about 109 ms | about 93 ms | 120 ms (p95 143-149) |
| after (`97a9d80`) | about 42 ms | about 30 ms | 57-65 ms (p95 76-78) |

This is one uncontrolled before/after pair on a mostly idle desktop, not a benchmark; it is the direction upstream describes (no frame waiting on later frames), not a measured size of the gain.

Not exercised on hardware: Cinnamon/X11 mapping, the decoder-failure recovery path, a second tablet.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/60_cleansing_and_upstream_sync.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/60_cleansing_and_upstream_sync.md)
