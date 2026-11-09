---
title: "38 Final overhaul: audit, MD3 UI, USB-bootstrapped onboarding, forced deploy"
sidebar:
  order: 25
  label: "38 Final overhaul: audit, MD3 UI, USB-bootstrapped onboardin"
---

Final overhaul: audit, MD3 UI, USB-bootstrapped onboarding, forced deploy

| Field | Value |
| :-- | :-- |
| **ID** | 38 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Audit

- `cargo clippy --workspace --all-targets`: 5 warnings at start (4 collapsible `if` in the Tauri crate, 1 `chunks_exact` in `host/src/input.rs` tests). Now 0.
- Tauri crate split from one 885-line `lib.rs` into `config.rs`, `adb.rs`, `daemon.rs`, `pairing.rs`. Removed: `ping_daemon`, `get_initial_view` (hard-coded test serials), `window.__*` test hooks, unused `dirs`/`anyhow` deps, `TabletProfile::display_name`, ignored `[profile.release]` in a non-root package, unused `CustomCheckbox/Select/Slider`, `App.css`, Vite/Tauri template assets.
- Android `lintDebug`: 0 errors, 10 warnings. Fixed the two that were code: obsolete `SDK_INT >= O_MR1` check, `mipmap-anydpi-v26` folder (minSdk is 27). Left: dependency-version notices, fixed orientation, `allowBackup`, monochrome icon (intentional or out of scope).

### Wi-Fi pairing

- `adb mdns check` and `adb mdns services` both work on this host with the default and `ADB_MDNS_OPENSCREEN=1` backends; avahi is active. No tablet was attached, so the service list is empty. **A real QR pairing was not exercised end to end.**
- Bugs found by reading the old flow, now fixed:
  - Paired with any `_adb-tls-pairing` service, not the one from our QR. Now matches the instance name `UScreen-XXXX`.
  - Declared success on `adb pair` exit code 0, which Android returns even for "Wrong password". Now requires "Successfully paired".
  - Ran `adb connect` against the pairing port. Connect uses a different port (`_adb-tls-connect._tcp`); it is now discovered over mDNS by IP.
  - Overlapping sessions raced on one boolean; replaced with a generation counter.
- Fallback inside the wizard: pairing IP:port, 6-digit code, optional connect IP:port. It opens automatically on stall (20 s), error or timeout (180 s). If the connect port cannot be discovered, the form asks for it.

### Build and deploy

- `build-all.sh` gained step 4/4: `pkill -9 -f uscreen-pro-gui`, `cp --remove-destination` to `~/.local/bin/{uscreen-pro-gui,uscreen}`, plus the APK to `~/.local/share/uscreen/app-debug.apk` (the GUI installs from there; the old copy was a day stale). It `cmp`s each file against `dist/` and prints timestamps. APK lookup now picks the newest candidate by mtime.
- `main.rs` sets `WEBKIT_DISABLE_COMPOSITING_MODE=1` first. Edition 2024 makes `set_var` `unsafe`; it runs before any thread exists. Effect on blur not measured; `/proc/<pid>/environ` does not show runtime `set_var`.

### Design (desktop and Android)

- Flat MD3 dark tokens, no gradients, glows, shadows or emoji. Cards 8px, inputs/buttons/switches 4px. 12px type, 28px controls, 36px rows.
- Radix primitives (Select, Switch, Slider, Dialog, Tooltip) styled with Tailwind; `@grenishrai/expressive` not used.
- Sliders: label left, value right, full-width track below. Layout padding on the sidebar and footer. Redundant header, "Concurrent Tablets Limit" and duplicate wireless buttons removed. Settings are single-column rows with tooltips.
- All requested settings are present (video, display, behaviour/system, app locking, autostart).
- Android: palette in `Theme.kt` matches desktop, onboarding gradient removed, card shapes capped at 8dp, hard-coded colours replaced with theme constants.

### Onboarding wizard (5 steps)

1. Connect: polls `adb devices`, advances when a USB device appears.
2. Debugging: shown for `unauthorized`; button fires `android.settings.APPLICATION_DEVELOPMENT_SETTINGS`. **Android rejects ADB commands from unauthorized hosts, so this button can fail exactly when it is shown**; the UI says so and falls back to manual steps. Advances when the state becomes `device`.
3. Install: installs `app-debug.apk` over USB automatically, retry on failure.
4. Wireless: QR `WIFI:T:ADB;S:<name>;P:<code>;;`, live status, manual fallback.
5. Finish: USB or Wi-Fi preference, nickname, saves profile.


## Result

### Verification

- Real app launched from `~/.local/bin/uscreen-pro-gui` after deploy: `screenshots/38_01_settings.png`.
- Wizard steps 1-5, device and health views were exercised in a browser against a mocked Tauri backend (scripted ADB states and pairing events), not against a tablet.
- Not verified: real pairing, APK install over USB, WebKit blur fix on a fractionally scaled display, the Android UI on a device (it compiles).

### Notes

- `~/.local/bin/uscreen-gui` still holds an older copy of the GUI; not touched.
- The `uscreen` daemon was replaced on disk but not restarted.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/38_claude_final_overhaul.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/38_claude_final_overhaul.md)
