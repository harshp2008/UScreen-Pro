---
title: "53b Setup errors, Host Offline, Debugging Disabled"
sidebar:
  order: 39
  label: "53b Setup errors, Host Offline, Debugging Disabled"
---

Already implemented earlier in this branch (see `53_dual_wireless_setup.md`): two tabs (Automatic / Tailscale),
"Connection Blocked by Router" amber box with MAC-randomization modal and Retry Setup, and
`pair_via_tailscale` (`adb connect <ip>:5555`, saves `tailscale_target_ip`). Not re-done. Not re-screenshotted this round.

| Field | Value |
| :-- | :-- |
| **ID** | 53b |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/53_screenshots/`; not published. |

## What was tested

### Tasks 1-3 (wizard Step 4)

Already implemented earlier in this branch (see `53_dual_wireless_setup.md`): two tabs (Automatic / Tailscale),
"Connection Blocked by Router" amber box with MAC-randomization modal and Retry Setup, and
`pair_via_tailscale` (`adb connect <ip>:5555`, saves `tailscale_target_ip`). Not re-done. Not re-screenshotted this round.

### Task 4 - Host Offline

- The video link is raw TCP, not WebSocket, so `host_shutdown` is a new packet type 2 on the video stream
  (payload `{"type":"host_shutdown"}`). `host/src/stream.rs`: each client task selects on the daemon shutdown
  watch, writes the packet, flushes, closes. Wired in `main.rs` and `session.rs`; `main.rs` waits 300 ms after
  signalling so the packet leaves before the process exits. Covers Ctrl+C, SIGTERM, tray and UI stop.
- `VideoReceiver.kt`: packet 2 sets `hostShutdown`, fires `onHostShutdown`, and the reconnect loop idles until
  `retryConnection()`.
- `Onboarding.kt` `WaitingScreen`: red error icon, "Host Offline", subtext, "Retry Connection". Also shown when
  no stream for >=10 s with a transport present (crash fallback).
- Verified on SM-X616B: stop via SIGINT -> logcat `Host announced shutdown` -> screen `s4_host_offline_shutdown.png`;
  no daemon at all -> `s2_host_offline_timeout.png`.

### Task 5 - Debugging Disabled

- Same screen: after >5 s waiting with `ADB_ENABLED == 0` shows "Debugging Disabled" + "Open Developer Options"
  (`ACTION_APPLICATION_DEVELOPMENT_SETTINGS`, fallback `ACTION_SETTINGS`). Priority: shutdown notice >
  debugging disabled > 10 s timeout > spinner, so it never masks a real Host Offline.
- NOT screenshotted. Turning USB debugging off from adb dropped the connection and the on-device restore script
  did not run, so the tablet stayed with debugging off and unreachable from here. Needs a manual re-enable.

### Build

`./build-all.sh` passes; binaries deployed to `~/.local/bin`; APK installed on the tablet.


## Result

not recorded as a separate section; see What was tested.

## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/53_setup_errors_offline_and_debugging_disabled.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/53_setup_errors_offline_and_debugging_disabled.md)
