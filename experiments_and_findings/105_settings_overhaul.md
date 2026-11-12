# 105: Settings overhaul (desktop app and tablet app)

Date: 2026-11-12. Branch `feat/settings-ui-overhaul`. Tablet SM-X616B (Galaxy Tab S9 FE+, USB), phone SM-A556E, host GNOME 46 on Wayland.
Related: record 104 (slow whole-desktop screenshots).

## What was changed (one commit per part)
| Part | Where |
|---|---|
| Rows wrap, columns follow the pane width (CSS grid auto-fit), shorter hints, durations in seconds, "Stylus · Dynamic" mode tab | `uscreen-pro-gui/src/components/ui/layout.tsx`, `PowerSettings.tsx`, `lib/options.ts` |
| Pen menu hold time 0.5 to 1.5 s; pen-cursor hide setting removed (host mechanism, config, both apps, docs) | `host/src/input/ghost/*`, `config/*`, tablet `HostLink.kt`, `SettingsSheet.kt` |
| `auto_open_app` = never / idle / always, replacing `auto_launch_app`; "idle" asks the tablet what is in front | `host/src/net/adb/app.rs` |
| Idle dim in Display mode only; tablet touches ping the host; pointer on tablet read from the compositor | `host/src/config/options/idle.rs`, `desktop/gnome/pointer.rs`, `MainActivity.kt` |
| Haptic strength, click sound, no-motor notice, left-tap feedback in native mode | `Haptics.kt`, `ClickSound.kt`, `NativeTapDetector.kt`, `TouchCapture.kt` |
| Touchpad style (clickpad / classic), middle button, classic tap-to-click | `touchpad_*` keys end to end, `TouchpadShape.kt`, `TapGate.kt`, `ActiveScreens.kt` |
| Autosave with Apply for stream settings and an Undo toast | `uscreen-pro-gui/src/App.tsx` |
| Draggable Bezier handles for the dim and undim curves | `components/CurveEditor.tsx` |
| Stylus tab in three cards, dropdowns | `PowerSettings.tsx` |
| Settings parity check | `scripts/tests/settings_parity.py` |
| Pen side button actions (undo, redo, copy, paste, cut, select all, open pen menu), host and both apps | `host/src/input/ghost/button_action.rs`, `config/clean.rs` |
| Interface size (automatic, or 50 to 300 %): set through Mutter's `ApplyMonitorsConfig`, live | `host/src/desktop/gnome/scale.rs`, `input/mapping.rs`, `input/server.rs`, both settings pages |
| "Settings updated" note on the tablet when a setting changes while it runs | `MainActivity.kt` |

## Findings that drove it
* **Left-click gave no feedback** because in native-gesture mode the computer's libinput makes the click from raw fingers, so the tablet never learns of it. Only the
  right-click corner (made on the tablet) called the haptic. The tablet now spots a short still touch itself.
* **This tablet has no vibration motor** as far as Android reports (`vibratorIds = []`, `hasVibrator()` false; forced `cmd vibrator_manager synced oneshot` was
  `ignored_unsupported`). Feedback there can only be sound or visual.
* **The undim bug**: the helper reads the pointer with `XQueryPointer`, which on Wayland stays at its last X position over native windows (xdotool 1464,540 against the
  compositor's 1088,527). On GNOME Wayland the daemon now polls `spawn-at query pointer` and the layout instead. The crossing itself was not exercised with a real mouse.
* **Dim in every mode** was designed (a test named for it); it is now Display mode only.

## Verified
* Host tests (204), Android unit tests (tap detector, tap gate, classic geometry, settings validation), `tsc`, clippy.
* In the browser preview: wrapping at 1100 and 1700 px, autosave ("Saved" with Undo) and the Apply button for a stream setting, the draggable curve handles.
* On the tablet over adb: the Touchpad style chips change the host's profile (`touchpad_style` written and kept for 20 s), and the touchpad screen draws the Classic pad with Left
  and Right buttons; Clickpad no longer draws a strip.
* The installed laptop binaries and the tablet's APK matched each build byte for byte.

## Not verified
* **Classic buttons and tap suppression on real touches.** Touches injected with `adb shell input` produced no events on the host, so the button and tap behaviour was only
  checked by unit tests (`TapGateTest`, `ClassicPadTest`).
* Haptic feel and the click sound, the undim on a real mouse crossing, the "home or lock screen" check beyond parsing real dumpsys output, and the phone (not connected).
* The desktop app's autosave on a real daemon (only the dev mock).

## Not verified (added later)
* The pen button actions with a real pen (unit tests cover the name to keys mapping only).
* **A first version of Interface size claimed a different physical size in the EDID and did nothing**: nothing it generated was ever used (no scaled EDID file appeared), and GNOME
  keeps the scale it chose for a monitor, so it was replaced by the Mutter call above. Its planning (parse, nearest supported scale, re-packing the monitors to the right, the busctl
  arguments) has unit tests; the live result is recorded below.
* The flash is a plain Android toast, not exercised on a device.

## Not done (planned)
Hold gestures with a progress ring, modifier combinations, the Quick menu (an icon panel with apps, key combos and scripts), per-app gestures. Record 104 found the screenshot
slowness is mostly fixed by capturing one monitor through the screen-cast portal (the flameshot-fast project), so Interface size is only an option.
