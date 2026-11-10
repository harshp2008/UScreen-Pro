---
title: "51 Touch-lock setting, Identify heads-up, tablet diagnosis, no Android onboarding, scroll"
sidebar:
  order: 36
  label: "51 Touch-lock setting, Identify heads-up, tablet diagnosis, "
---

Touch-lock setting, Identify heads-up, tablet diagnosis, no Android onboarding, scroll

| Field | Value |
| :-- | :-- |
| **ID** | 51 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### Changes

| # | Area | Files | Change |
|---|------|-------|--------|
| 1 | Touch on lock screen | `Prefs.kt`, `TouchCapture.kt`, `ui/ConnectionScreen.kt`, `ui/SettingsSheet.kt`, `MainActivity.kt` | New setting **Touch on Lock Screen** (default **off**). Locked and setting off: a full-screen scrim consumes every touch and `TouchCapture.setInputBlocked(true)` stops finger, pen and hover forwarding (and lifts anything pressed). Setting on, or device unlocked: no overlay, input flows. The row is greyed with the tooltip *"Your device does not support touch injection on the lock screen."* (also shown as visible text) when `lockScreenTouchSupported()` is false (API < 27, no `setShowWhenLocked`). Keyguard state is re-read in `onResume`. Note `minSdk` is 27, so the greyed state cannot occur today. |
| 2 | Identify notification | `adb.rs` (`identify_device`), new `IdentifyReceiver.kt`, manifest, `AddDeviceWizard.tsx` | `cmd notification post` has **no priority flag** (`post --help` on the tablet: only `-t -i -I -S -c -u`), so `-p high/critical` does not exist and its notifications never pop up. The heads-up now comes from the app: `adb shell am broadcast -n com.uscreen/.IdentifyReceiver --es name '<name>'` posts `UScreen: <name>` / "This is your target device!" on an IMPORTANCE_HIGH channel. Receiver is exported but guarded by `android.permission.DUMP` (shell/system only). `POST_NOTIFICATIONS` declared and granted with `pm grant`. Fallback when the app is not installed yet: `cmd notification post -S bigtext -t 'UScreen: <name>' ...` (name in title, no heads-up). Args are now shell-quoted: `adb shell` re-splits on spaces, so the old unquoted `"UScreen Pro"` title was split. Name = wizard model, else `ro.product.model`. |
| 3 | Tablet connection | `adb.rs` | See diagnosis below. `ro_serial` and `installed_app_version` probes raised 800 ms -> 3 s so a slow device is not reported as "no app installed". |
| 4 | Android onboarding | `ui/Onboarding.kt`, `ConnectionScreen.kt`, `SettingsSheet.kt`, `Prefs.kt` | The 4-step wizard, `StepIndicator`, "Restart Full Setup Wizard", "Review Setup Guide" button and `hasCompletedOnboarding` are gone. `OnboardingScreen` is now `WaitingScreen` (spinner, diagnostic text, "Help Me Fix It" troubleshooter). |
| 4b | Auto-launch | `adb.rs` (`install_apk`, `launch_app`) | After `adb -s <serial> install` succeeds: `adb -s <serial> shell am start -n com.uscreen/.MainActivity`; failure is non-fatal and reported in the install log. |
| 5 | Skip install | `AddDeviceWizard.tsx` | Button removed from Step 3. ("Skip for now (USB Only)" on the wireless step is a different control and stays.) |
| 6 | Settings scroll | `ui/SettingsSheet.kt` | `ModalBottomSheet` replaced by a full-screen `Dialog` (`usePlatformDefaultWidth = false`) with a close button; content is `fillMaxSize` + `verticalScroll`. No draggable surface, so a drag anywhere (including the blank margin) scrolls. |

### Tablet diagnosis (SM-X616B, serial TABLET_SERIAL, Android 16 / SDK 36)

* `adb devices -l`: `TABLET_SERIAL device usb:3-1 ... model:SM_X616B`. **Not** `unauthorized`/`offline`. The phone was not attached.
* logcat: no adbd/auth/USB-permission errors. The `Auth` lines are Google account `BadAuthentication` token noise, unrelated.
* Play Protect did **not** revoke anything: `Finsky VerifyApps ... Verification package=com.uscreen ... verdict 0` (clean) at install time; `package_verifier_user_consent=1`. Play Protect never touches ADB keys, so no revoke/replug is needed. (If a tablet ever does show `unauthorized`: Developer options > Revoke USB debugging authorizations, replug, tap Allow.)
* `adb shell pm list packages` on this tablet prints `Shell does not have permission to access user 150` (Samsung Secure Folder). Harmless for the commands the backend uses (`pm path`, `dumpsys package`, `install` all target user 0).
* Probe timings are ~0.1 s, so the old 800 ms limits were not the cause here; the widened limits are a safeguard only. **No tablet-specific backend fault was reproduced.**

### Verification

* `cargo test --lib`: 6 passed (new `identify_title_and_quoting`). `tsc --noEmit` clean. `./build-all.sh` exit 0; `~/.local/bin/uscreen-pro-gui` identical to `dist/`.
* `adb -s TABLET_SERIAL install -r dist/app-debug.apk` Success; `am start -n com.uscreen/.MainActivity` opens straight to "Waiting for UScreen desktop..." (no wizard).
* Identify: `am broadcast` returned `result=-1, data="posted"`; `dumpsys notification` shows channel `uscreen_identify` importance=4 (HIGH); screenshot shows the heads-up "UScreen: Tab S9 F... / This is your target device!".
* Settings: opened full screen; a swipe starting at x=60 (blank left margin) scrolled the list; "Touch on Lock Screen" present and off.

* **Lock overlay (tablet unlocked by the user, then locked again with the app brought to the front over the keyguard, `isKeyguardShowing=true`):**
  * setting off: dimmed screen plus the red "Device Locked - Touch Disabled" banner;
  * setting switched on from the settings page (the gear stays reachable above the overlay): overlay and banner gone while the keyguard was still showing; switched back off afterwards.


## Not verified

* That finger touches really stop reaching the host while the overlay is up, and that they pass with the setting on: no stream was running, so only the overlay state was observed.
* The Identify fallback (app not installed) and the GUI-driven install + auto-launch were not run end to end; `am start` was run by hand with the same arguments.
* The tablet is left locked by this test.

## Sources

- [`experiments_and_findings/51_fix_touch_lock_and_scrolling.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/51_fix_touch_lock_and_scrolling.md)
