---
title: "58 Six GUI / latency bugs"
sidebar:
  order: 41
  label: "58 Six GUI / latency bugs"
---

The GUI is `uscreen-pro-gui/` (Tauri); there is no `gui/src/main.rs`.

| Field | Value |
| :-- | :-- |
| **ID** | 58 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/58_screenshots/`; not published. |

## What was tested

| # | Finding | Fix | Proof |
|---|---|---|---|
| 1 Stop Daemon closes the window | The handler (`daemon.rs::stop_daemon`) was already correct. The tray launched the GUI as a plain child of the daemon, i.e. inside `uscreen.service`'s cgroup; `systemctl stop` (`KillMode=control-group`) killed it too. Reproduced with a stand-in service: plain child died, `systemd-run` child survived. | `tray.rs::open_settings` starts the GUI as its own transient unit (`systemd-run --user --collect`), plain spawn only as a fallback. | Real tray "Show Window" (D-Bus click) → GUI in `uscreen-pro-gui-<ts>.service`; `systemctl --user stop uscreen` → daemon inactive, GUI pid unchanged, button reads "Start Daemon" (`58_screenshots/1_*`). The click on the GUI button itself could not be synthesised (uinput pointer events are not delivered to the window), so the identical `systemctl` call was used. |
| 2 Host Offline vs waiting | `probeHostSocket()` (connect, read: timeout = live listener, EOF = adb reverse with nothing behind it) existed but nothing called it; "offline" was a 10 s timer. | `Onboarding.kt`: 1 Hz probe inside `repeatOnLifecycle(STARTED)` (stops on onStop, resumes on onStart); 3 consecutive failures → Host Offline; a success after offline clears it and calls `retryConnection()`. Host `stream.rs`: a probe that hangs up silently is no longer logged as a bad login. | Daemon stopped → tablet shows Host Offline (`2a`); daemon started → tablet streams again with no tap (`2b`, 33 fps overlay). |
| 3 High-FPS logic | Not removed. `-framerate` follows the configured fps; `--max-performance` (16 ms idle keepalive instead of 33 ms) is still in `capture.rs`/`evdi_helper.c` but only engages when `powerprofilesctl get` is `performance` (this machine: `power-saver`) — the design from experiment 14. | None needed. | `ps`: `evdi_helper --fps 90`, `ffmpeg … -framerate 90`, config `fps = 90`. |
| 4 Mouse lag | Not USB: encoder → tablet screen (wire + tablet decode) is 22–25 ms. The host encode stage at 2560x1600 dominates: capture-ready → screen p50 ≈ 120 ms, host stages ≈ 99 ms. Synthetic encoder probe (`scripts/enc_latency_probe.py`, the daemon's exact ffmpeg args): 2560x1600 ≈ 57–66 ms, 1920x1080 ≈ 22 ms, 1280x800 ≈ 22 ms; `-low_power` unsupported on this GPU, `-compression_level`/`-quality`/`-fps_mode passthrough` no real gain. | `uscreen doctor --latency [--secs N]` (sweeps a uinput mouse across both screens, asks the daemon for percentiles via new `LATENCY <secs>` control command backed by a rolling sample store in `latency.rs`). The desktop mouse never reaches the daemon, so what is measured is the frame path the cursor rides on. | Real daemon: 2560x1600 stream 120.3/132.0 ms (p50/p95), host 99.2 ms; stream_scale=2 (1280x800) 63.9/84.2 ms, host 45.2 ms; back to full 119.1/131.3 ms. Note: the desktop mode itself was not changed to 1920x1080 (that is an EVDI hotplug); `stream_scale=2` and the synthetic 1920x1080 run stand in for it. |
| 5 HEVC toggle | It only ever affected `hevc_vaapi` (`-b_depth 1 -sei 0 -aud 0`); NVENC is always run zero-latency, H.264 has no such options. `-low_delay_b` does **not exist** for hevc_vaapi (`Unrecognized option 'low_delay_b'`), so it was not added; `-async_depth 1` was already always set. | GUI shows the toggle only for `hevc_vaapi`; ffmpeg reads the flag fresh at each spawn and the GUI restarts the pipeline when it changes. | `ps`: ON → `-c:v hevc_vaapi -async_depth 1 -b_depth 1 -sei 0 -aud 0`; OFF → `-c:v hevc_vaapi -async_depth 1`. Browser render: toggle absent for NVENC (and H.264), present for Intel/AMD HEVC. |
| 6 Software cursor option | Upstream (`majmichu1/UScreen`) never calls `evdi_enable_cursor_events`, i.e. libevdi's default composited cursor; its cursor handlers are empty stubs. That matches our working default (composited cursor + boundary nudge, RCA 56 §8). | New setting `software_cursor` (default off): "Use Software Mouse Cursor (Slower, more compatible)". On → helper gets `USCREEN_CURSOR_EVENTS=0` and `USCREEN_BOUNDARY_NUDGE=0` (plain composited cursor, no X11 nudge window); off → the fast path. Saving restarts the pipeline. | Helper `environ`: off → none; on → `USCREEN_BOUNDARY_NUDGE=0 USCREEN_CURSOR_EVENTS=0` and log `Boundary nudge disabled (software cursor mode)`. |

Deviations from the brief: Task 5 flag `-low_delay_b` omitted (invalid); Task 6's ON/OFF mapping differs because the default already composites the cursor (the difference is the nudge window); Task 4 mouse timestamps are frame-path, not desktop-mouse. User config was edited temporarily for tests and restored (verified identical).


## Result

not recorded as a separate section; see What was tested.

## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/58_six_bug_fixes.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/58_six_bug_fixes.md)
