# Experiment 35: Institutional Deep Codebase Cleansing, Structural Modularization, and Pre-Tauri Technical Debt Reduction

**Date:** 2026-11-06  
**Author:** Principal Systems Engineer & Code Quality Auditor  
**Workspace:** `~/Documents/GITHUB PROJECTS/UScreen`  
**Base Commit:** `b306058` (`refactor/deep-codebase-cleansing`)  
**Status:** In Progress — Active Refactoring Protocol  

---

## 1. Executive Summary & Objective

Prior to initiating the upcoming Tauri UI migration, UScreen requires a comprehensive technical debt reduction sprint. Over successive rapid development cycles spanning Experiments 01 through 34, multiple critical components have evolved into monolithic files:
- `host/src/main.rs` exceeds 2,400 physical lines, mingling CLI argument parsing, background ADB device discovery loops, active display session state machines, IPC sockets, and config file handlers.
- `android/app/src/main/java/com/uscreen/MainActivity.kt` approaches 2,000 lines, conflating the core Activity lifecycle, permission requests, deep link handling, and three distinct Jetpack Compose screen trees (Onboarding / Troubleshooter, Settings Sheet, Connection Screen).
- `host/evdi/evdi_helper.c` exceeds 1,400 lines with unformatted styling, legacy dead code, and implicit POSIX signal handling behavior that warrants clear architectural documentation.
- The build orchestration (`Makefile`, `build.rs`) and documentation lack explicit modular documentation describing all pipeline targets and runtime dependencies.

### Critical Behavioral Directive
**Zero functional regression.** Refactoring is strictly invariant: zero behavior changes, zero protocol drift, identical network and IPC signatures, and full binary compatibility across both host and Android targets. All transformations must be verified continuously via:
- `cargo check` and `cargo clippy -- -D warnings`
- `cargo test --workspace`
- `./gradlew assembleDebug`
- `make all`

---

## 2. Baseline Codebase Audit (Task 0)

An initial audit using `cloc` (v2.10) on branch `refactor/deep-codebase-cleansing` established the following line counts across languages and primary monolithic target files:

### 2.1 Language Totals
```
-------------------------------------------------------------------------------
Language                     files          blank        comment           code
-------------------------------------------------------------------------------
Rust                            24           1030           1618          11565
Markdown                        40           1251              0           6722
Kotlin                           7            277            555           3007
C                                4            224            198           1603
Bourne Shell                     8             81            245            560
Python                          11            129            109            519
HTML                             1             25              7            418
JavaScript                       1             29              6            200
YAML                             4             14              5            178
Gradle                           3             16              5            125
C/C++ Header                     1             24              4            115
make                             1             21             11            114
XML                              6              5             18             88
DOS Batch                        1             21              2             69
SVG                              3              9             11             67
TOML                             3              3              5             37
-------------------------------------------------------------------------------
SUM:                           126           3168           2809          25442
-------------------------------------------------------------------------------
```

### 2.2 Largest Target Files Baseline
| File | Blank Lines | Comment Lines | Code Lines | Total Lines | Target Actions |
|---|---|---|---|---|---|
| `host/src/main.rs` | 154 | 250 | 2,026 | 2,430 | Extract `adb.rs`, `session.rs`, `config_handler.rs`; rustdoc; clippy |
| `android/.../MainActivity.kt` | 108 | 129 | 1,720 | 1,957 | Modularize Compose UI (`Onboarding.kt`, `SettingsSheet.kt`, `ConnectionScreen.kt`) |
| `host/evdi/evdi_helper.c` | 142 | 179 | 1,157 | 1,478 | `clang-format` (LLVM), document POSIX signal loop, purge dead macros/code |
| `Makefile` | 21 | 11 | 114 | 146 | Document targets, verify build dependencies, clean up legacy targets |
| `host/build.rs` | 5 | 0 | 43 | 48 | Document compilation steps, EVDI link requirements, and build invariants |

---

## 3. Systematic Execution Log

### 3.1 Task 0: Branch Initialization & Baseline Line Count
- Created branch: `refactor/deep-codebase-cleansing` from base commit `b306058`.
- Measured baseline line counts across `host/src/main.rs` (2,430 total lines), `android/.../MainActivity.kt` (1,957 lines), `host/evdi/evdi_helper.c` (1,478 lines).

### 3.2 Task 1: Rust Backend Cleansing (`host/src/`)
- **Diagnostic Audit**: Executed `cargo fmt --check` and `cargo clippy --workspace -- -D warnings`. Identified 11 clippy violations across `host` and 3 across `gui`, including derivable defaults, map key discarding, suspicious file open options without truncation, chunks_exact vs as_chunks, and formatting reference redundancy.
- **Decomposition of Monolithic `main.rs`**:
  - Extracted ADB transport detection, reverse port tunneling, APK launcher intents, and device state discovery into `host/src/adb.rs` (651 lines).
  - Extracted multi-device session state machine, display slot orchestration, and pause/resume lifecycle handling into `host/src/session.rs` (889 lines).
  - Extracted GNOME app-lock state synchronization, Unix socket IPC control server, and PID tracking into `host/src/config_handler.rs` (303 lines).
  - Reduced `host/src/main.rs` from 2,430 lines to 806 physical lines.
- **Documentation**: Added comprehensive Rust doc-comments (`///`) to all public functions and data types explaining architectural rationale and behavioral invariants.
- **Clippy & Code Formatting**: Resolved all workspace clippy warnings with `-D warnings` enabled; applied standard rustfmt across all workspace crates.
- **Verification**:
  - `cargo check`: PASSED (0 warnings)
  - `cargo clippy --workspace -- -D warnings`: PASSED (0 warnings)
  - `cargo test --workspace`: PASSED (43/43 host tests, 4/4 GUI tests)
  - `./gradlew assembleDebug`: PASSED (exit code 0)

### 3.3 Task 2: C Helper Cleansing (`host/evdi/evdi_helper.c`)
- **Formatting Standardization**: Applied `clang-format` using LLVM style across `host/evdi/evdi_helper.c`.
- **Architectural Documentation**: Added detailed K&R/Doxygen-style documentation above POSIX signal handlers (`handle_signal`, `handle_throttle`) detailing:
  - `SIGTERM`/`SIGINT`: Orderly event-loop exit, thread join, condvar broadcast, boundary window teardown, and EVDI handle release.
  - `SIGUSR1`: 1 FPS keep-alive standby throttle to eliminate CPU/GPU bus contention when client is inactive or in background.
  - `SIGUSR2`: Resuming active frame cadence and triggering `mark_all_dirty()` across all triple-buffer slots to guarantee immediate full-frame keyframe delivery upon reconnection.
- **Dead Code & Warning Elimination**:
  - Removed dead/shadowed variable `request_period_ns` in `run_event_loop()`.
  - Expanded `drm_path` buffer to 512 bytes in `find_evdi_device()` to eliminate potential snprintf truncation warnings.
  - Verified with `gcc -Wall -Wextra -O3`: 0 warnings, clean compilation.
- **Verification**:
  - `cargo check`: PASSED
  - `cargo test --workspace`: PASSED
  - `./gradlew assembleDebug`: PASSED

### 3.4 Task 3: Android UI Cleansing (`android/`)
- **Diagnostic Audit**: Executed `./gradlew lintDebug` and `compileDebugKotlin`. Identified unused parameter warnings in `getLocalWifiIp(context)` and `notePenActivity(vw, vh)`.
- **Decomposition of Monolithic `MainActivity.kt`**:
  - Extracted theme definitions and color tokens into `android/app/src/main/java/com/uscreen/ui/Theme.kt` (31 lines).
  - Extracted onboarding progression cards, developer options guidance, connection diagnostics, and interactive troubleshooter into `android/app/src/main/java/com/uscreen/ui/Onboarding.kt` (544 lines).
  - Extracted live settings bottom sheet, bitrate/FPS sliders, display orientation toggles, preferred transport selector, and wireless pairing setup into `android/app/src/main/java/com/uscreen/ui/SettingsSheet.kt` (564 lines).
  - Extracted main SurfaceView video viewport, stats overlay chip, version mismatch banner, device locked banner, and graphics tablet blanking into `android/app/src/main/java/com/uscreen/ui/ConnectionScreen.kt` (451 lines).
  - Reduced `MainActivity.kt` from 1,957 physical lines down to 520 lines, focusing strictly on Activity lifecycle, hardware refresh rate management, and input hover event dispatching.
- **Documentation**: Added comprehensive KDoc documentation to all public and major internal composables, diagnostic state evaluators, and system check helpers.
- **Verification**:
  - `./gradlew lintDebug`: PASSED (0 errors)
  - `cargo check`: PASSED
  - `cargo test --workspace`: PASSED
  - `./gradlew assembleDebug`: PASSED




