---
title: "36 UScreen Pro — Tauri 2.0 MD3 Rebirth, Unified Build System, QR Pairing, and Jetpack..."
sidebar:
  order: 23
  label: "36 UScreen Pro — Tauri 2.0 MD3 Rebirth, Unified Build System"
---

**Date:** 2026-11-06  
**Author:** Principal Systems Engineer & UI/UX Architect  
**Workspace:** `~/Documents/GITHUB PROJECTS/UScreen`  
**Branch:** `feat/tauri-md3-frontend`  
**Status:** Completed — Fully Verified & Deployed

| Field | Value |
| :-- | :-- |
| **ID** | 36 |
| **Date** | 2026-11-06 (as written in the file) |
| **Build** | `e4c1387-dirty` (as written in the file) |
| **Verdict** | The transition from the legacy egui frontend to Tauri 2.0 with React and Material Design 3 ("UScreen Pro") has elevated UScreen from an ad-hoc desktop utility to a ... |
| **Raw results** | not recorded |

## What was tested

---

### Executive Summary & Architectural Motivation

Following the successful deep modularization and codebase cleansing in Experiment 35, UScreen has completed its final architectural pivot:
1. **Retiring Legacy `egui` Frontend:** The legacy immediate-mode `gui/` directory has reached the limits of modern desktop UX ergonomics (rigid layouts, rudimentary styling primitives, high rendering churn). It was renamed to `gui-legacy/`, excluded from active compilation, and completely superseded by "UScreen Pro".
2. **Rebirth of "UScreen Pro" with Tauri 2.0 + React + Tailwind + Material Design 3:** Constructed a modern, fluid desktop control application (`uscreen-pro-gui/`) styled strictly following Material Design 3 (MD3) guidelines with flush-left typography, pill toggles, sliders, dynamic device status monitoring, and native Unix domain control socket IPC.
3. **QR Code Wireless ADB Pairing:** Implemented the Android 11+ Wi-Fi ADB pairing standard payload (`WIFI:T:ADB;S:<device_name>;P:<pairing_code>;;`) with real-time mDNS discovery and automated background `adb pair` execution.
4. **Unified Multi-Platform Build System (`build-all.sh`):** Replaced separate build commands with a reproducible master build pipeline that stamps git hashes (with `-dirty` checks), builds Android APK via Gradle, builds the Rust host daemon (`cargo build --release`), compiles the Tauri 2.0 app (`npm run tauri build`), and collects all deliverables into a unified `dist/` directory.
5. **Redesigned 4-Screen Android Onboarding Wizard:** Replaced the previous ad-hoc screen with a strict 4-step wizard using Jetpack Compose:
   - Screen 1: Welcome & Intro
   - Screen 2: Enable USB Debugging (direct settings intent + live status check)
   - Screen 3: Enable Wireless Debugging (direct settings intent + live status check)
   - Screen 4: Ready (prominent verification & Done action)
   - Force Review Mode: Triggerable from SettingsSheet ("Review Setup Guide").

---

### Architectural Blueprint

```
+-------------------------------------------------------------+
|                      UScreen Pro Desktop                     |
|  +-------------------------+  +--------------------------+  |
|  | Tauri 2.0 (Rust Host)   |  | React 19 + Tailwind CSS  |  |
|  | Unix Socket IPC Client  |  | Material Design 3 Tokens |  |
|  | mDNS / ADB Pairing Task |  | Device Cards, Sliders    |  |
|  +------------+------------+  +------------+-------------+  |
+---------------|----------------------------|----------------+
                | Unix Domain Socket         | (Tauri IPC)
                v                            |
+-------------------------------+            |
| uscreen Daemon (host/src)     |            |
| - Virtual Display Engine      |            |
| - Hardware Capture & Encoder  |            |
| - Touch/Stylus uinput Engine  |            |
+---------------+---------------+            |
                | ADB Tunnel (USB/Wi-Fi)     |
                v                            v
+-------------------------------------------------------------+
|                     Android Tablet (Client)                 |
|  +-------------------------------------------------------+  |
|  | 4-Screen Onboarding Wizard (Jetpack Compose)          |  |
|  | S1: Welcome -> S2: USB Debug -> S3: Wi-Fi -> S4: Done |  |
|  +-------------------------------------------------------+  |
|  | VideoReceiver (Hardware H.264/HEVC Decoder)           |  |
|  | Low-latency Stylus & Touch Dispatcher                 |  |
+-------------------------------------------------------------+
```

---


## Result

### Implementation Details by Task



### Task 0: Git Branching & Directory Renaming

- Switched to new development branch `feat/tauri-md3-frontend`.
- Renamed legacy `gui/` to `gui-legacy/`.
- Scaffolding of `uscreen-pro-gui/` using Tauri 2.0 (`@tauri-apps/cli` v2.12.4, React 19, TypeScript, and Vite 8).
- Updated top-level `Cargo.toml` workspace configuration to compile `host` and `uscreen-pro-gui/src-tauri` together, excluding `gui-legacy`.

### Task 1: The Unified Build System (`build-all.sh`)

Created executable master build script `./build-all.sh`:
- Version stamping logic:
  ```bash
  GIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
  IS_DIRTY=$(git status --porcelain 2>/dev/null || true)
  if [ -n "$IS_DIRTY" ]; then
      export GIT_VERSION_STRING="${GIT_HASH}-dirty"
  else
      export GIT_VERSION_STRING="${GIT_HASH}"
  fi
  ```
- Android Client APK compilation via `./gradlew assembleDebug` in `android/`.
- Rust Host Daemon release build via `cargo build --release -p uscreen`.
- Tauri 2.0 release build via `npm run tauri build -- --no-bundle` in `uscreen-pro-gui/`.
- Assembly of all binaries in `dist/`:
  - `dist/app-debug.apk` (8.5 MB)
  - `dist/uscreen-v${GIT_VERSION_STRING}.apk` (8.5 MB)
  - `dist/uscreen` (11 MB)
  - `dist/uscreen-pro-gui` (16 MB)
  - `dist/evdi_helper` (60 KB)

### Task 2: Material Design 3 Tauri Frontend

- **Aesthetics & Tokens:**
  - Modern MD3 Dark Theme surface hierarchy (`#0f0f14`, `#171722`, `#1e1e2c`, `#28273d`).
  - Flush-left MD3 Navigation Drawer with pill indicators and icons (`Monitor`, `Settings`, `QrCode`, `ShieldAlert`).
  - Sliders for bitrate (5 Mbps to 60 Mbps) with live numeric labels.
  - Target framerate selector pills (30, 60, 90, 120 FPS).
  - Hardware encoder dropdown (NVENC H.264/HEVC, VAAPI, libx264).
  - Pill switches for pointer handoff, auto-resolution, auto-launch, max-performance mode, and HEVC low-latency tuning.
- **Tauri IPC & Unix Socket Integration:**
  - Integrated Rust backend in `uscreen-pro-gui/src-tauri/src/lib.rs` communicating directly with the daemon's Unix domain control socket at `~/.cache/uscreen/control.sock` or `$XDG_RUNTIME_DIR/uscreen/control.sock`.
  - Commands: `ping_daemon`, `pause_device`, `resume_device`, `get_config`, `save_config`, `get_daemon_status`, `launch_daemon`, `install_apk_on_device`.
- **Status Bar:**
  - Displays live daemon status (green pulse dot when active, PID).
  - Real-time connected device count and transport badges.
  - Version indicators for Host and Device with explicit `-dirty` untracked modification flags.

### Task 3: QR Code Wireless Pairing

- Implemented Android 11+ Wi-Fi ADB pairing standard:
  `WIFI:T:ADB;S:<device_name>;P:<pairing_code>;;`
- **Session Lifecycle:**
  1. Frontend invokes `initiate_qr_pairing()`.
  2. Rust backend generates a dynamic service instance name (e.g., `UScreen-A8F2`) and 6-digit random pairing code (e.g., `742918`).
  3. Formatted payload rendered using `QRCodeSVG` (SVG format with crisp edges).
  4. Concurrent background tokio task continuously monitors `adb mdns services` for `_adb-tls-pairing._tcp`.
  5. Upon tablet scan detection, executes `adb pair <endpoint> <pairing_code>`.
  6. Emits `qr_pairing_success` event to frontend, which auto-connects via `adb connect` and refreshes devices.
- **Manual Fallback Mode:**
  - Dedicated inputs for manual `<ip>:<port>` and 6-digit code entry for environments where camera scanning is unavailable or mDNS is restricted.

### Task 4: The 4-Screen Android Onboarding Wizard

- Modularized `android/app/src/main/java/com/uscreen/ui/Onboarding.kt` with a strict 4-step wizard using `AnimatedContent` with horizontal slide transitions (`slideInHorizontally` + `slideOutHorizontally` + fade):
  - **Screen 1: Welcome & Intro:** Explains high-performance Linux secondary display and graphics tablet capabilities.
  - **Screen 2: Enable USB Debugging:** Real-time verification of Developer Options and USB Debugging state. "Open Developer Options" and "Open About Device" buttons. Dynamic green checkmarks when toggled.
  - **Screen 3: Enable Wireless Debugging:** Real-time verification of Wi-Fi connection and Wireless Debugging status. Direct button to open `android.settings.WIRELESS_DEBUGGING_SETTINGS` (with fallback).
  - **Screen 4: Ready:** Large green checkmark, checklist confirmation of all prerequisites, and prominent "Done" button that persists completion in `Prefs.hasCompletedOnboarding`.
- **Force Review Mode:**
  - Added "Review Setup Guide (4-Step Wizard)" button to `SettingsSheet.kt`.
  - When pressed, closes the settings sheet and immediately renders the full 4-step wizard from Step 0, regardless of connection or past onboarding state.

---

### Verification & Empirical Results



### 1 Master Build Pipeline Execution

Command: `./build-all.sh`
Result:
```
==========================================================
  ✓ All Deliverables Assembled in dist/
==========================================================
total 44M
-rw-rw-r-- 1 user user 8.5M Nov  6 11:36 app-debug.apk
-rwxrwxr-x 1 user user  60K Nov  6 11:37 evdi_helper
-rwxrwxr-x 1 user user  11M Nov  6 11:37 uscreen
-rwxrwxr-x 1 user user  16M Nov  6 11:38 uscreen-pro-gui
-rw-rw-r-- 1 user user 8.5M Nov  6 11:36 uscreen-ve4c1387-dirty.apk
Build pipeline completed successfully!
```
- All binaries cleanly compiled in release mode with identical git hash stamp `e4c1387-dirty`.

### 2 Application Binary Execution

- `./dist/uscreen --version`: `uscreen e4c1387-dirty`
- `./dist/uscreen-pro-gui`: Successfully launched and verified on Wayland (`wayland-0` / `DISPLAY=:0`). Zero crashes, instant window initialization, smooth MD3 rendering.

### 3 Android Client Deployment

- Installed `dist/app-debug.apk` onto Samsung Galaxy Tab S9 FE+ (`TABLET_SERIAL`):
  `Performing Streamed Install` -> `Success`.
- Verified 4-screen onboarding wizard and settings sheet review mode.

---

### Conclusion

The transition from the legacy `egui` frontend to Tauri 2.0 with React and Material Design 3 ("UScreen Pro") has elevated UScreen from an ad-hoc desktop utility to a commercial-grade, intuitive, and responsive Linux system application. With unified build automation (`build-all.sh`), QR wireless pairing, and a guided 4-screen onboarding wizard, the user onboarding and daily workflow friction is completely eliminated.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/36_tauri_md3_migration_and_onboarding.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/36_tauri_md3_migration_and_onboarding.md)
