---
title: "66 Settings reorganisation, exclusive input modes, curve editor, brightness fixes"
sidebar:
  order: 44
  label: "66 Settings reorganisation, exclusive input modes, curve edi"
---

Date: 2026-11-08. Branch `feat/upstream-sync`. Tablet SM-X616B over USB, host GNOME on Wayland.

| Field | Value |
| :-- | :-- |
| **ID** | 66 |
| **Date** | 2026-11-08 (as written in the file) |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### What changed

| Task | Where | Notes |
|---|---|---|
| 1. Settings in four collapsible cards (Display & Input, Connection, Power & Battery, Advanced) | `ui/SettingsSheet.kt` (rewritten) | Cards with a tappable header; Display & Input starts open. |
| 2. Pairing guide on its own screen, button only while no Wi-Fi address is known | `ui/PairingGuideScreen.kt`, `HostConfig.wifiConfigured` | The host puts `wifi_configured` in every `config_update`. Buttons are `#047857` with white text (5.5:1). The theme's own primary is light mint in dark mode, so the explicit colours are used. |
| 3. `input_mode = display / touchpad / stylus` | `host/src/config.rs` (`InputMode`), `options.rs`, `daemon.rs`, `session.rs`, Android + GUI segmented control | Old `pen_only` / `touchpad_mode` are migrated once and no longer written (`pen_only` is mirrored for older tools). Per-device override works. |
| 4. Touchpad cursor | `input/uinput.rs`, `input/events.rs`, `TouchCapture.kt` | `INPUT_PROP_POINTER` set on the `UScreen Touchpad` device (see below); a `wake` message nudges the pointer +1/-1 px when a finger comes down. |
| 5. Number inputs and curve editor | `components/CurveEditor.tsx`, `lib/curves.ts`, `NumberInput` | Presets Linear, Ease in, Ease out, Ease in-out, Exponential; Custom with x1 y1 x2 y2, live SVG preview, Save as Preset, delete. Saved presets live in `config.toml` as `[[curve_presets]]`; they reach the tablet as `bezier:x1,y1,x2,y2` and become a `PathInterpolator`. |
| 6. Battery saver locking is optional | `battery_saver_lock` (default off) | Off: dim to 10% and stop retrying. On: also `lockNow()`; the tablet asks for Device Admin when the switch goes on. |
| 7. Wake on touch; lock-screen brightness | `ConnectionScreen.kt` (overlay), `MainActivity.kt` (`applyBrightnessPolicy`) | Brightness override is released while locked, in `onPause` and in `onStop`, and re-applied (5%, or battery saver's 10%) when unlocked. |
| 8. Parity and sync | `PowerSettings.tsx`, `HostConfig`, `options.rs` | Every Android setting is now also a GUI row and travels both ways: input mode, touch input, black screen in stylus mode, immersive, show stats, orientation, theme, keep connected in background, keep display alive when locked, touch on lock screen, battery saver, device admin lock, idle dimming. |

Parity gaps found: show stats, touch input, black screen in stylus mode, orientation, theme, keep connected in background, keep display alive when locked and touch on lock screen existed on the tablet only. They are now GUI rows with global default and per-device override. Android has no auto-launch or security token controls; its Advanced card says so.

### The cursor fix, honestly

The task blamed the device being registered as a touchscreen/touchpad. The device did not have `INPUT_PROP_DIRECT` or `INPUT_PROP_TOUCHPAD` (there is no such uinput property); it had no property at all. Now `/proc/bus/input/devices` shows `B: PROP=1` (bit 0, `INPUT_PROP_POINTER`) and `EV=7` (SYN, KEY, REL), no DIRECT. `libinput debug-events` on that device shows `+1/0` then `-1/0` at the start of a swipe after an idle second, followed by the normal motion.

**I did not confirm that the cursor is now visible.** A host screenshot with the pointer included was inconclusive and also captured the desktop, so I deleted it. Please swipe on the tablet in Touchpad mode and look. If the cursor is still hidden the cause is something else.

### Verification

- Host: `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p uscreen` 86 passed (new: input-mode override, curve resolution, tablet-to-override mapping, legacy migration, idle dim outside Display mode).
- Android: `testDebugUnitTest` 21 passed (7 `HostLinkTest`, 14 gesture tests): settings mapping, Bezier parsing and clamping.
- GUI: `tsc` clean; in the dev preview I enabled the dim-curve override, chose Custom, typed 0.1 / 0.9 / 0.2 / 1, saved it as "Snappy"; the mock IPC received `save_curve_presets` with it and "Snappy" appeared in the Undim curve dropdown (screenshots in `screenshots/66_*`). Not clicked in the real Tauri window.
- Tablet, display mode, idle dim 10 s / 5 s linear / 0.5 s exponential: brightness fell 0.24, 0.19, 0.14, 0.10, 0.05 in one-second steps. A tap on the dimmed screen brought it back (0.05, 0.053, 0.058, 0.072, 0.099, 0.195, 0.475 within about 0.5 s).
- Lock while dimmed: at 0.05, `input keyevent 26` then 224 left no window with a brightness override and `isKeyguardShowing=true`; system brightness was 255.
- Switching `input_mode` in `config.toml` reached the tablet within a second, and the pipeline came down in touchpad mode (no ffmpeg).


## Not verified

- **Unlocking.** The tablet has a PIN. I could not unlock it, so "re-apply 5% after unlock until touched" and the PIN pad itself being bright after a real unlock are untested; only the release-while-locked half was observed.
- **Android screenshots.** With the keyguard up, injected taps never reached the app, so I could not open its Settings or pairing screens. No screenshots of them exist. Unlock the tablet and open the gear to see them.
- Segmented control, section layout and the pairing screen were compiled but not looked at on the device.
- Stylus mode behaviour is unchanged from graphics-tablet mode (the pen is absolute, mapped to the computer's own screen). The task text said "relative pen"; I did not change that.
- Idle dim outside Display mode: found during this work that Touchpad and Stylus never dimmed, because the pointer flag stays "on the tablet" without the helper. Fixed (pointer counts as away outside Display mode) and covered by a unit test; not run on the tablet.
- The tablet was left locked with `touch_on_lock_screen` toggled on temporarily; config values were restored from a backup afterwards.

## Sources

- [`experiments_and_findings/66_mega_ui_power_and_input_fixes.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/66_mega_ui_power_and_input_fixes.md)
