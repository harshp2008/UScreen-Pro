---
title: "50 Target locking and UX fixes"
sidebar:
  order: 35
  label: "50 Target locking and UX fixes"
---

Target locking and UX fixes

| Field | Value |
| :-- | :-- |
| **ID** | 50 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### Changes

| # | Area | File(s) | Change |
|---|------|---------|--------|
| 1 | Android uninstall spinner | `ui/ConnectionScreen.kt` (`UnpairedBanner`) | Tap shows a spinner ("Uninstalling…") and disables the button at once. The uninstall dialog is launched with `ACTION_UNINSTALL_PACKAGE` + `EXTRA_RETURN_RESULT` through `rememberLauncherForActivityResult`, so the spinner clears when the dialog is answered (uninstalled or cancelled), or immediately if no uninstaller can handle the intent. Banner text is now white on its dark scrim (was theme `OnSurface`, unreadable in light mode). |
| 2 | Pairing guide | `AddDeviceWizard.tsx`, new `ManualPairDialog.tsx`, `App.tsx` | Guide text uses only `text-foreground` / `text-muted-foreground`; the `Note` tones and the mDNS warning no longer put `text-warning`/`text-destructive` on tinted backgrounds. Manual pairing (pairing IP:port, 6-digit code, connect IP:port) moved out of the accordion into its own dialog. The QR tab strip is gone (QR is the guide; manual is the dialog). The guide ends in one state-driven control: device already has a `wifi_address` → green **Paired** badge and no button; a USB device is attached → **Pair Automatically (via USB)** (starts the USB flow); no USB device → **Pair Manually (via Code)** (opens the dialog). `App` passes `wirelessSerials` (profiles with a `wifi_address`). |
| 3 | Android settings scroll | `ui/SettingsSheet.kt` | The scroll container is now `fillMaxWidth()` + `weight(1f, fill = false)` + `verticalScroll`, with the padding inside it, and the sheet opens fully expanded (`skipPartiallyExpanded = true`). A half-open sheet turned the first drag into "expand the sheet", and a width-wrapped Column left the side margins outside the scroller. Dragging on the blank margin now scrolls the list. |
| 4 | Transport "Auto" | `AddDeviceWizard.tsx` | Android Settings and desktop Device Settings **already had Auto** (Auto first, default `TRANSPORT_AUTO` / `"auto"`). The place that really offered only USB and Wi-Fi was the wizard's Finish step, which also defaulted to USB. It now offers Auto / USB cable / Wi-Fi in a 3-column row, default **Auto**, and every path (skip, wireless pair) leaves it on Auto. |
| 5 | Wizard target confusion | `AddDeviceWizard.tsx`, `adb.rs` | Root cause: with several USB devices the wizard silently took `candidates[0]`. Now, with more than one USB device attached (and the wizard not pinned to one), `usb` stays null until the user presses **Setup this device** on a row; `Next` is disabled until then. Paired devices are listed as "Already added". The chosen serial is stored (`pickedSerial` then `chosenSerial`) and is the only serial used afterwards; unplugging it raises the disconnect banner instead of switching to the other device. "Choose a different device" undoes the pick before moving on. |
| 5b | Backend `-s` audit | `adb.rs` | Install, wireless bootstrap, Wi-Fi settings, developer-options, wireless-info and identify commands all pass `-s <serial>` (`require_serial` refuses an empty one). `open_developer_options` and `open_wireless_debugging_settings` took `Option<String>`, so `None` ran adb with no `-s`; they now require a serial. Left without `-s` on purpose, because adb has no per-device form for them: `adb pair`, `adb connect`, `adb disconnect`, `adb mdns services`, `adb devices`, `start-server`, `kill-server`. |
| 6 | Identify | `adb.rs` (`identify_device`), wizard | New IPC command: `adb -s <serial> shell cmd notification post -S bigtext -t "UScreen Pro" uscreen-identify "This is your target device!"` plus a best-effort buzz (`cmd vibrator_manager synced -f oneshot 500 255`). **Identify** buttons sit on every picker row and next to the selected device in the status box. Result is a toast ("Notification sent and the device vibrated", or the failure). Note `cmd notification post` needs a TAG before the text. |

### Verification

* `./build-all.sh` exit 0; deployed binaries `cmp`-identical; `adb -s PHONE_SERIAL install -r dist/app-debug.apk` Success. (Plain `adb install -r` fails here when the same phone shows up on USB and on Wi-Fi, so `-s` is needed.)
* Rust: `cargo test --lib serial_tests` 2 passed (empty/blank serial refused by `require_serial`, `identify_device`, `open_developer_options`, `open_wireless_debugging_settings`).
* Wizard, preview with a phone and a tablet both unpaired (`?scenario=multi`): picker listed both, `Next` disabled; **Identify** on the second row sent `identify_device {serial: TABLET_SERIAL}`; **Setup this device** on the tablet, then every later IPC (`install_apk_on_device`, `bootstrap_wireless_over_usb`) carried `TABLET_SERIAL`, never the phone's serial.
* Guide: no USB → "Pair Manually (via Code)" → separate dialog with the three inputs; USB attached → "Pair Automatically (via USB)"; single tablet with a `wifi_address` → green "Paired" badge and no button (dark mode screenshot legible).
* Phone (SM-A556E): the identify commands post a notification on exactly that device (seen in its shade); Settings sheet scrolls when dragging from blank margin; uninstall button showed "Uninstalling…" disabled on the frame right after the tap and the system `UninstallLaunch` dialog opened; cancelling it returned to the app and left it installed.


## Not verified

* The phone has a secure lock screen that came up while the uninstall dialog was open, so the dialog itself was cancelled from adb (`force-stop` of the package installer), not tapped. The spinner clearing after a cancel is therefore inferred from the result callback (the app was resumed afterwards), not seen on screen.
* The real machine has one physical phone attached (USB + Wi-Fi entries for the same device), so the picker was exercised in the mock; the Identify commands were run against the real phone by hand with the same arguments the command uses.
* The vibration is best effort and may do nothing on devices without a motor; the notification is the guarantee.
* Test scaffolding added to `devMock.ts`: `?scenario=multi`, `?scenario=wifi`, `identify_device`.

## Sources

- [`experiments_and_findings/50_target_locking_and_ux_fixes.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/50_target_locking_and_ux_fixes.md)
