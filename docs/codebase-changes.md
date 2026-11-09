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
| `scripts/tests/run.sh` | 53 passed | 53 passed |
| `npx tsc --noEmit` | clean | clean |
| `./gradlew assembleDebug testDebugUnitTest` | passes | passes |
| `node --check extensions/uscreen-app-lock/extension.js` | ok | ok |
| `bash -n` on every `*.sh` | ok | ok |

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

## Waiting for a decision (not deleted)

These look unused but are files, so they are left alone until you decide.

| File | What I found |
|---|---|
| `host/evdi/evdi_capture_test.c`, `host/evdi/evdi_test2.c` | EVDI experiment programs; not in the `Makefile`, not in any workflow, not referenced by any document |
| `host/evdi/evdi_grab_test.c` | same, and it also contains a pasted Markdown explanation of `evdi_grab_pixels` in the middle of the C source |
| `android/app/src/main/java/com/uscreen/ui/MorphButton.kt` | the `MorphButton` composable and `PillToSquareShape` are not used by any screen |
| `scripts/test-gen-edid.py` | not run by CI or the `Makefile`; only mentioned by an experiment record |

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
