---
title: "69 Stylus profiles, touchpad gestures, haptics, warnings"
sidebar:
  order: 45
  label: "69 Stylus profiles, touchpad gestures, haptics, warnings"
---

> **Superseded by `72_input_engine_rewrite.md`.** The stylus profile system and the gesture engine described here were removed: they did not hold up in real-world use.

| Field | Value |
| :-- | :-- |
| **ID** | 69 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | - S Pen SDK is vendored, not a dependency. |
| **Raw results** | not recorded |

## What was tested

Branch `feat/upstream-sync`, nothing committed. Tablet used: Galaxy Tab S9 FE+ (SM-X616B, Android 16).


## Result

### What was built

| Task | Where | Result |
|---|---|---|
| 1. Stylus isolation | `TouchCapture.kt` (`stylusMode`), `events.rs` | Fingers (and mouse tool types) are ignored in the app, and the host drops `touch` messages in Stylus mode as a second line. Pen and eraser stay absolute. |
| 2. Profile engine | `host/src/stylus_profile.rs` | `StylusProfile` trait, `StylusEvent`, `HostAction`, `StylusProfileRegistry` (loads `~/.config/uscreen/stylus_profiles/*.json`, hot-reloads), built-in `StandardProfile` and `SamsungSPenProfile`, custom JSON profiles. |
| 2. Samsung source | `android/spen-remote/` (vendored), `SamsungSPen.kt` | S Pen Remote button and air motion forwarded as `stylus_event`s. Runs only on Samsung and only while the Samsung profile is active. |
| 2. Profile Manager | GUI `StylusProfileManager.tsx`, tablet `StylusProfileManager.kt` | Create / edit / delete / Test / pick active profile in both. |
| 3. Execution | `uinput.rs` (`UScreen Keyboard`, `run_action`) | Mouse click, scroll notch, key combos with Ctrl/Shift/Alt/Meta. |
| 4. Gestures | `TouchpadGestures.kt`, `events.rs` | Pinch = Ctrl+wheel, rotate = Alt+wheel, 3-finger tap = middle (already existed), 4 fingers = Meta+PageUp, 5 = Meta+PageDown. |
| 5. Haptics, zones | `Haptics.kt`, `ConnectionScreen.kt` | `touchpad_haptics` (default on), `right_click_method` (`bottom_right_zone` default / `two_finger_tap`), faint strip drawn on the tablet. |
| 6. FPS overlay | `ConnectionScreen.kt`, `SettingsSheet.kt` | Overlay and its switch exist in Display mode only (the overlay was already hidden in the other two; the switch was not). |
| 7. Warnings | `AddDeviceWizard.tsx` | Amber EXPERIMENTAL badge on Tailscale plus the beta note; collapsible "Connection Failed?" box with the 3-step MAC randomization fix. |

Parity: the three new settings (`stylus_profile`, `touchpad_haptics`, `right_click_method`) go through the same global-default / per-device-override path as every other setting (`config.rs`, `options.rs`, `config_update`, `HostConfig.kt`, GUI `PowerSettings.tsx`). Profiles are the same JSON files edited from both UIs. The GUI does not parse them itself: it calls the daemon binary (`stylus-profiles`, `stylus-profile-save`, `stylus-profile-delete`, `stylus-test`), so the tablet, the GUI and the daemon share one parser.

### Decisions that deviate from the brief

- **S Pen SDK is vendored, not a dependency.** `david-allison/S-Pen-Remote-Open-SDK` exists and is Apache-2.0, but publishes no Maven artifact or release. Its `lib/` module is copied into `android/spen-remote/` (rev 4719445, sources untouched; its build file drops three androidx/material dependencies the sources never import). Attribution added to `THIRD_PARTY_LICENSES.md`.
- **No `evdevil`.** The raw uinput layer works and is tested; it only needed a keyboard device (`UScreen Keyboard`, created on first use like the touchpad mouse).
- **Samsung "Conditionally compiled" is a runtime guard**: Android cannot compile per device. `SamsungSPen.isSupportedDevice()` plus the SDK's own brand check; the profile chip is hidden on non-Samsung tablets.
- **Circle gestures are not implemented.** The SDK exposes only the button and air-motion deltas. Swipes (left/right/up/down) are classified on the host from the accumulated motion (`SWIPE_MIN = 30` SDK units, unverified against a real S Pen; the number is a guess and the first thing to tune).
- **Air tap fires on release**, so a tap can be told from a swipe.
- **Pen roll** uses `AXIS_ORIENTATION` (30 degrees of change per step, hover only). That axis is the pen's azimuth, so ordinary hand movement changes it; map it only if you want that.
- **Two-finger scroll vs pinch vs rotate** is decided once, from per-finger travel. Android reports the fingers one at a time, so the decision waits until both have moved (or one has travelled 36 px).
- A tap in the bottom-right strip is a right click; with that method on, a two-finger tap does nothing (the setting picks one).

### Verification

`cargo test -p uscreen`: 99 pass (new: action parsing, registry, tracker, key frames, profile-to-evdev tests). `cargo clippy` clean. Android `testDebugUnitTest`: 32 pass (new: pinch, rotate, 4/5 finger, zone, settings, catalog). `npx tsc` clean. `./build-all.sh` exit 0, binaries and APK deployed, APK installed over USB.

End-to-end (`scripts/e2e_stylus_touchpad.sh`, a fake tablet client on the real input socket plus the real tablet, evdev capture of the virtual devices; restores `config.toml` and deletes its test profile on exit):

- Profile `copy-pen` with `button_primary = ctrl+c`, active: pen button 5/6 produced `KEY_LEFTCTRL down, KEY_C down/up, KEY_LEFTCTRL up` and no `BTN_STYLUS`. A focused Tk entry with selected text put `uscreen-copy-test` on the clipboard (wl-paste and xclip).
- Stylus mode: two real `adb input tap` finger taps and one forged `touch` message: 0 `BTN_TOUCH` on `UScreen Touch`. Note the host drop would also hide an app that still sent touches; the app-side half rests on the code and the host-side half is what was observed.
- Touchpad mode on the real tablet: tap at (600,400) gave `BTN_LEFT`; tap at (1900,1500), inside the strip, gave `BTN_RIGHT`. Screenshot `screenshots/69_tablet_touchpad_zone_no_stats.png` (strip drawn, no stats overlay although `show_stats = true`).
- Gesture messages: pinch 40 gave Ctrl + 2 wheel notches, rotate 30 gave Alt + wheel -1, workspace prev/next gave Meta+PageUp / Meta+PageDown.
- GUI (browser preview with mock IPC, not the Tauri window): created `Copy pen`, Test ran, saved, listed and auto-activated (`screenshots/69_gui_stylus_profile_manager.jpg`); wizard badge and MAC box (`69_gui_wizard_*.jpg`).


## Not verified

- **Haptics.** This tablet reports no vibrator to apps (`cmd vibrator_manager list`: "No vibrator found", `vibratorIds = []`), so `Haptics.click` correctly does nothing here. The click path that triggers it did run (the `BTN_LEFT` / `BTN_RIGHT` above).
- **Real S Pen**: side button, air button, air motion and roll were not exercised; nothing can press them over adb. The host half is covered by the tests above, the S Pen SDK half has never run on hardware.
- **Pinch, rotate, 3/4/5-finger taps from real fingers**: `adb input` cannot make multi-touch. Covered by `TouchpadGesturesTest`, and the host side by the gesture messages above.
- **Tablet-side Stylus Profile Manager UI** compiles and its parsing is unit-tested, but it was not opened on the tablet. The desktop GUI inside the real Tauri window was not opened either (browser preview only).
- Two early runs of the e2e script failed because of a test bug (a duplicate `stylus_profile` key made the daemon read defaults) and a capture race; the final run above is the passing one.
- The e2e run sent a real Ctrl+C to the focused desktop window and two workspace switches (one undone), by design of the test.

## Sources

- [`experiments_and_findings/69_stylus_profiles_and_gestures.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/69_stylus_profiles_and_gestures.md)
