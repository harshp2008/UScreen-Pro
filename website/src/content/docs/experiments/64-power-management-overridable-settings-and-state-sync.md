---
title: "64 Power management, overridable settings and state sync"
sidebar:
  order: 43
  label: "64 Power management, overridable settings and state sync"
---

Date: 2026-11-08. Branch `feat/upstream-sync`. Tablet: SM-X616B over USB, host GNOME on Wayland.

| Field | Value |
| :-- | :-- |
| **ID** | 64 |
| **Date** | 2026-11-08 (as written in the file) |
| **Build** | not recorded |
| **Verdict** | - Touchpad mode replaces the old tablet-only touchpad switch. |
| **Raw results** | not recorded |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### What was built

| Area | Where |
|---|---|
| Config schema: global default + `[tablets.<id>]` override for `touchpad_mode`, `immersive_mode`, `battery_saver_on_disconnect`, `idle_dim_enabled`, `idle_dim_timeout_secs`, `dim_curve`, `undim_curve`, `dim_duration_ms`, `undim_duration_ms` | `host/src/config.rs`, `uscreen-pro-gui/src-tauri/src/config.rs`, `src/lib/types.ts` |
| Per-slot live options registry, `config.toml` watcher (500 ms mtime poll), tablet-to-host overrides, idle-dim state machine | `host/src/options.rs` |
| `config_update` on connect and on every change, `setting` message from the tablet | `host/src/input/{server,events,protocol}.rs` |
| Touchpad mode stops the pipeline (gate in `daemon.rs` / `session.rs`; keep-alive no longer keeps it up) | `daemon.rs`, `session.rs`, `capture/supervisor.rs` |
| Pointer-on-tablet reporting (`POINTER_IN_TABLET 0\|1` on the helper's stdout) | `host/evdi/evdi_helper.c`, `capture/helper.rs` |
| `dim_screen` / `undim_screen` (non-sticky notice) | `net/notices.rs`, `options.rs` |
| Android: `HostConfig`, `ReconnectPolicy` (2 s / 120 s), `FadeCurve`, `ScreenDimmer`, `ChargingStatus`, `ScreenLock` + device-admin receiver | `HostLink.kt`, `PowerSupport.kt`, manifest, `res/xml/device_admin.xml` |
| Android wiring: config apply, link watchdog, battery saver, Retry, touchpad static screen, settings sheet rows | `MainActivity.kt`, `TouchCapture.kt`, `VideoReceiver.kt`, `ui/*` |
| GUI: "Power and touch" and "Idle Power Saving" sections, shared by Settings and device pages (override switch per row); power tooltip on the device Touchpad row | `components/PowerSettings.tsx`, `views/*` |
| `build-all.sh` now runs `make build-helper` (the helper was never rebuilt or deployed by it) | `build-all.sh` |

### Behaviour decisions

- **Touchpad mode replaces the old tablet-only touchpad switch.** It is now the synced setting and it means "no video": the gate drops the EVDI helper and ffmpeg for that slot only.
- **Static frame.** The tablet draws "Touchpad Mode Active" itself when the host announces touchpad mode. Nothing is encoded or sent as a frame; an encoded frame would need the encoder running.
- **Reconnect.** The socket and video loops retry every 2 s. The activity's watchdog treats "control socket not open" as down; 120 s of it flips "Host Offline". A host-shutdown packet no longer shows it at once. With battery saver off the retries and the host probe carry on forever.
- **Battery saver (on, after 120 s).** All retry loops stop (`autoReconnect`, `autoRetry`, `connect()` refuses), brightness 10%, keep-screen-on and wake lock released, `lockNow()` via device admin. Retry re-arms everything and restores system brightness. No Shizuku path.
- **Idle dim.** Timer armed while the pointer is off the tablet and restarted by any tablet input (touch, pen, touchpad). Input while dimmed undims. If the helper never reports (software cursor mode) the pointer counts as "on the tablet", so nothing dims. Undim goes to 1.0 as specified, and stays there.
- **Exponential curve** is `(2^(10t) - 1) / 1023`, a slow start with a steep finish, for both dim and undim.
- **Tablet toggles** (touchpad, immersive, battery saver) are sent as `setting` and stored as that device's override.

### Verification (real tablet)

- Daemon and APK updated by `./build-all.sh`; the tablet's first `config_update` arrived on connect.
- **Touchpad mode** (override in `[tablets.DEVICE_SERIAL]`, global `keep_alive = true`): within about a second the supervisor logged "Tablet is not a screen", ffmpeg and evdi_helper exited, `pgrep` found neither. The tablet showed the static screen (screenshot). Swipes with `adb shell input swipe` produced a steady `POINTER_MOTION` stream from the `UScreen Touchpad` uinput device (about 10 ms spacing, 21-23 px steps, `libinput debug-events`). I did not watch the on-screen cursor.
- **Idle dim**, 10 s timeout, dim 5 s linear, undim 0.5 s exponential, sampled from `dumpsys window` (`sbrt`): dim steps of about 0.07 per second from 0.38 to 0.05 over 5 s. After a tap: 0.05, 0.053, 0.073, 0.14, 0.48, 1.0 within about 0.5 s.
- **Live sync**: editing `config.toml` changed the tablet within about a second (`Host config applied` in logcat for touchpad, idle dim, immersive and battery saver). Not tested: toggling a switch on the tablet and watching the host file change.
- **Daemon stopped**: 63 failed connection attempts in 120 s (about one per 2 s), no "Host Offline" before it. At 120 s the log shows "Host unreachable for 120 s" then "Battery saver", brightness 0.1, "Host Offline" screen with the battery-saver text. `lockNow()` was accepted (`isKeyguardShowing=true`, `lockNow pending` in the system log).

### Checks

`cargo clippy -p uscreen --all-targets -- -D warnings` clean; `cargo test -p uscreen` 82 passed (6 of them new, in `options`); `./gradlew testDebugUnitTest` 19 passed (5 new in `HostLinkTest`); `tsc` clean; `./build-all.sh` exit 0.


## Not verified

- **The screen did not turn off.** This tablet's screen timeout is "never" (`screen_off_timeout` 2147483647), and One UI applies a pending `lockNow` when the display sleeps, so the display stayed on at 10% behind the keyguard. With a normal timeout it would sleep.
- **Retry on the tablet was not exercised.** The keyguard is secure and I did not enter the PIN; `wm dismiss-keyguard` did nothing. The Retry logic has no device test. Only the 120 s policy has unit tests.
- Found during the test and fixed: after the lock, `onStart` reconnected on its own, defeating "user must tap Retry"; `connect()` now refuses while `autoReconnect` is off. The fix was built and installed but not re-run through another 2-minute cycle. A stale stats overlay in touchpad mode was also fixed.
- The device admin was granted for the test with `dpm set-active-admin` (works on this device). `dpm remove-active-admin` was refused, so it may still be active; remove it in Settings, Security, Device admin apps.
- Pointer tracking depends on the helper's X11 boundary thread (Xwayland pointer). Pointer-away was inferred from the dim firing about 10 s after touchpad mode was turned off; I did not move the cursor onto the tablet.
- Charging text: the unit tests cover the formatting; I did not open the sheet and read it on the tablet.
- GUI: type-checked and built, not opened or clicked.

## Sources

- [`experiments_and_findings/64_power_management_and_overrides.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/64_power_management_and_overrides.md)
