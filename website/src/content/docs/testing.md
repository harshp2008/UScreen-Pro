---
title: "Testing"
---

Run these from the repository root. None needs root, a tablet or the network (the build steps need the toolchains installed).

| Check | Command | What it covers |
| :--- | :--- | :--- |
| host | `cargo test` in `host/` | the daemon: config, protocol, pen state machine, installer queue, adb parsing, desktop backends' parsers |
| desktop app, Rust side | `cargo test` in `uscreen-pro-gui/src-tauri` | config merge rules (last change wins), adb helpers, known-hosts editing |
| desktop app, TypeScript | `npx tsc --noEmit` in `uscreen-pro-gui` | types |
| installer | `scripts/tests/run.sh` | see [Installer tests](/UScreen-Pro/installer-tests/) |
| Android | `./gradlew assembleDebug testDebugUnitTest` in `android/` | the build, and unit tests of the touchpad engine, reconnect policy and settings model |
| extension | `node --check` on each file and `node extensions/uscreen-app-lock/tests/check-versions.mjs` | syntax and the Shell version lists; the extension itself cannot run outside GNOME Shell |
| shell scripts | `bash -n` on every `*.sh` | syntax |
| C helper | `make build-helper` | it compiles and links |
| lint | `cargo clippy --all-targets`, `cargo fmt --check` | style and common mistakes |

## What no test covers

A real tablet, a real EVDI device, a real compositor, the packages, and the Shell extension in a live session. Those are checked by hand, and the records under Findings say when.

## Sources

- `docs/development.md` ("Running and testing")
- `.github/workflows/build.yml`, `.github/workflows/installer.yml`
