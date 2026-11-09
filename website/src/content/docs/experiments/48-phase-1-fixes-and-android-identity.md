---
title: "48 Phase 1 fixes and Android identity"
sidebar:
  order: 33
  label: "48 Phase 1 fixes and Android identity"
---

Phase 1 fixes and Android identity

| Field | Value |
| :-- | :-- |
| **ID** | 48 |
| **Date** | not recorded |
| **Build** | `399f5c6` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/48/`; not published. |

## What was tested

### Phase 1

| # | Item | Result |
|---|------|--------|
| 1 | Sidebar order and empty state | `Sidebar.tsx`: Paired Devices first, New Devices below. With no paired device the header reads `Paired Devices (0)` with a muted `None` under it. Screenshots: `48/phase1_real_app_sidebar_ordering.png` (real app, 1 paired + 1 new), `48/phase1_sidebar_empty_state.png` (real app, empty config: `Paired Devices (0)`, `None`, then `New Devices (2)`). |
| 2 | "Add device" restarted the wrong device | Root cause: the wizard did `entries.find(e => !e.is_wifi)`, i.e. the first USB device adb listed, whichever sidebar entry was clicked. With a paired tablet and a new phone both on USB it picked the tablet, and the install (`adb install -r`) restarted the tablet's app. Fix: the sidebar passes the clicked serial (`onAddDevice(serial)`), the wizard pins to it (`targetSerial`), paired serials are never offered, and with several unpaired candidates a picker is shown and the choice is frozen before Install. Backend: `install_apk` and `bootstrap_wireless_over_usb` refuse an empty serial (`adb::run` drops `-s` for an empty string); every adb call in those paths carries `-s <serial>`. Verified with two devices (see below). |
| 3 | Forget Device feedback on Android | Host: `forget_device` -> daemon `FORGET` now broadcasts `{"type":"unpaired"}` over the slot's input WebSocket and waits 600 ms before tearing the session down. Android: `TouchCapture` handles it, stores `unpaired_by_host` in `Prefs`, and `UnpairedBanner` covers the screen with "Unpaired by Host. This device is no longer connected." and an **Uninstall App** button (`ACTION_DELETE`, `package:com.uscreen`; `REQUEST_DELETE_PACKAGES` added to the manifest). The flag is cleared when the host greets the app again. |
| 4 | Old updater removed | `UpdateCheck.kt` deleted (it called `api.github.com/repos/majmichu1/UScreen/releases/latest`). Removed the `updateAvailable` state, the `onStart` check, the "Update x available" toast and the Settings sheet update card. No code in the app fetches anything from that repo now. (The one-time "Open GitHub" thank-you note still opens the repo in a browser when the user taps it; it makes no request on its own.) |
| 5 | Separate Wireless step | Wizard is now Prepare Device -> Connect USB -> Install -> **Wireless** -> Finish. The Wireless screen has "Set up wireless" and **Skip for now (USB Only)**; wireless no longer starts by itself. |
| 6 | Skipped wireless | Skipping (or a failed setup) saves `wireless_configured = false` (`TabletProfile` in both the GUI and the host, so the daemon's own config rewrites keep it; absent means configured, so older profiles are unchanged). On the device page the preferred-transport select and the Wi-Fi address field are disabled and a **Configure Wireless Mode** button re-opens the wizard on the Wireless step for that serial; on success it saves `wifi_address` and `wireless_configured = true`. |
| 7 | Granular progress | Backend events `install_progress` ("Found old APK", "Removing old APK", "Installing new APK") and `pair_progress` ("Starting ADB server", "Pairing...", "Connecting..."), shown as a step list on the Install and Wireless screens. The wizard install now uninstalls an existing `com.uscreen` first (an APK signed with another key, such as the original project's release, would otherwise fail with a signature mismatch); the daemon's update path still uses `-r` so app data is kept. |
| 8 | Snapshot | `./build-all.sh` exit 0, old GUI killed, `~/.local/bin/uscreen-pro-gui` replaced (byte-identical to `dist/`), committed as `399f5c6` "fix: onboarding logic, device categorization, and update removal". The commit also contains the earlier uncommitted work on this branch (installer queue, notices, forget, experiments 47), because the Phase 1 changes sit on top of it. |


## Result

### Verification of the Add device fix (two devices on USB)

Browser run of the dev mock (`?scenario=two`: paired SM-X616B `TABLET_SERIAL`, new SM-A556E `PHONE_SERIAL`), clicking the new phone in the sidebar and walking the wizard; the IPC log recorded:

```
install_apk_on_device {"serial":"PHONE_SERIAL"}
save_device_profile  {"serial":"PHONE_SERIAL","profile":{"label":"SM-A556E","preferred_transport":"usb","wireless_configured":false}}   (after Skip for now)
```

The paired tablet's serial never appears in an install or bootstrap call. Not done: a run of the native window clicking through the wizard against the real adb devices, and a live Forget Device against a connected device (the unpaired signal is compiled and unit-wired, not exercised end to end).

### Phase 2

| # | Item | Result |
|---|------|--------|
| 9 | Theme | `Theme.kt` now carries the desktop `.dark` tokens from `uscreen-pro-gui/src/index.css`, converted from oklch to sRGB: primary `#34D399`, on-primary `#00382A`, background/surface `#0C1513`, card `#141D1B`, sidebar `#18211F`, foreground `#DBE5E0`, muted `#3A4A45`, muted-foreground `#B8CBC4`, success `#8FD6A3`, warning `#F2C46D`, destructive `#FFB4AB`. Shapes follow the desktop radius scale (4/6/8 dp, 12 dp for large and extra-large). Typography: sans-serif (Roboto), 12 sp body, 11 sp captions, 20 sp medium titles. Leftover purple/indigo literals in `Onboarding.kt` and the overlay scrims were re-tinted. |
| 10 | Name and icon | `res/values/strings.xml` defines `app_name` = "UScreen Pro", used by the manifest label. Icons come from the desktop source `icons-src/uscreen-pro.svg`: `npx tauri icon` for the legacy `ic_launcher.png` / `ic_launcher_round.png` in every `mipmap-*`, plus an adaptive icon (emerald `#059669` background, the monitor artwork as `ic_launcher_foreground.png`, scaled into the safe zone). Manifest has `icon` and `roundIcon`. |
| 11 | Final build and deploy | `./build-all.sh` exit 0 (APK, daemon, Tauri GUI; deployed and byte-checked in `~/.local/bin` and `~/.local/share/uscreen`). `adb -s PHONE_SERIAL install -r dist/app-debug.apk` -> `Success` on the SM-A556E (the new device; `-s` because two devices are attached). The paired tablet was not touched by hand. Screenshots: `48/phase2_android_app_theme.png` (green theme), `48/phase2_android_icon_and_name.png` (App info: "UScreen Pro" and the new icon; the launcher page itself was not captured because it shows a personal wallpaper). |

### Where the spec and the desktop disagree

The task gave `Primary = #10B981` and `Surface = #061E14` as examples of the desktop colours. The desktop app's dark scheme actually uses primary `#34D399` (`#10B981` is only the seed and chart-3) and background `#0C1513`. The Android theme follows the desktop file, since the goal was an exact match; swapping in the two example values is a two-line change in `Theme.kt` if they were intended.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/48_phase1_fixes_and_android_identity.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/48_phase1_fixes_and_android_identity.md)
