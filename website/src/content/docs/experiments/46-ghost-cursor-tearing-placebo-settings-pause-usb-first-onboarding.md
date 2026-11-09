---
title: "46 Ghost cursor/tearing, placebo settings, Pause, USB-first onboarding"
sidebar:
  order: 31
  label: "46 Ghost cursor/tearing, placebo settings, Pause, USB-first "
---

**Hypothesis.** The refactor flipped `evdi_enable_cursor_events` to `true`, burning the cursor into the stream.
**Method.** `git log -S` per commit, then an A/B on the real tablet (`adb screencap`) with a synthetic uinput mouse
circling on the EVDI output (`scratchpad/motion.py`), plus a 60 fps `testsrc2` window moved onto the tablet output.
**Raw findings.**
- History: `false` was added in 11d6b65, changed to `true` on purpose in adc91ac (before the refactor); 5821a6d only reformatted it. Hypothesis rejected.
- EVDI semantics: `true` = cursor delivered as `cursor_set`/`cursor_move` events and left OUT of the grabbed framebuffer; `false` = libevdi composites it INTO every grabbed frame.
- `true`: helper `total grabs` stayed at 25 during motion, no cursor in the stream (`tablet_cursor_events_true.png`).
- `false`: 11.2 grabs/s during motion, cursor sprite visible at the right edge of the tablet (`tablet_cursor_events_false.png`).
- Moving content (`tablet_clean_motion.png`): continuous diagonal sweep, intact bars, no horizontal tear.
**Conclusion.** The flag stays `true` (the user-requested `false` does the opposite of "not burned in"). A comment now records this.
Tearing was not reproduced. Real defect fixed: SIGUSR2 called `mark_all_dirty()` from signal context while the writer thread
rotates the three dirty masks under `g_swap_mutex`, which can leave stale rows after a reconnect. The handler now sets
`g_full_refresh_req`; the writer applies it under the lock. Frame order: one writer thread, monotonic `g_fifo_seq`, triple buffer; reordering is not possible.
Note: `build-all.sh` does not compile the C helper; use `make build-helper`.

| Field | Value |
| :-- | :-- |
| **ID** | 46 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/screenshots/46/`; not published. |

## What was tested

### Part 1: cursor and tearing

**Hypothesis.** The refactor flipped `evdi_enable_cursor_events` to `true`, burning the cursor into the stream.
**Method.** `git log -S` per commit, then an A/B on the real tablet (`adb screencap`) with a synthetic uinput mouse
circling on the EVDI output (`scratchpad/motion.py`), plus a 60 fps `testsrc2` window moved onto the tablet output.
**Raw findings.**
- History: `false` was added in 11d6b65, changed to `true` on purpose in adc91ac (before the refactor); 5821a6d only reformatted it. Hypothesis rejected.
- EVDI semantics: `true` = cursor delivered as `cursor_set`/`cursor_move` events and left OUT of the grabbed framebuffer; `false` = libevdi composites it INTO every grabbed frame.
- `true`: helper `total grabs` stayed at 25 during motion, no cursor in the stream (`tablet_cursor_events_true.png`).
- `false`: 11.2 grabs/s during motion, cursor sprite visible at the right edge of the tablet (`tablet_cursor_events_false.png`).
- Moving content (`tablet_clean_motion.png`): continuous diagonal sweep, intact bars, no horizontal tear.
**Conclusion.** The flag stays `true` (the user-requested `false` does the opposite of "not burned in"). A comment now records this.
Tearing was not reproduced. Real defect fixed: SIGUSR2 called `mark_all_dirty()` from signal context while the writer thread
rotates the three dirty masks under `g_swap_mutex`, which can leave stale rows after a reconnect. The handler now sets
`g_full_refresh_req`; the writer applies it under the lock. Frame order: one writer thread, monotonic `g_fifo_seq`, triple buffer; reordering is not possible.
Note: `build-all.sh` does not compile the C helper; use `make build-helper`.

### Part 2: settings and Pause

Cause: the daemon read `config.toml` only on tablet connect, so GUI saves were never applied; `paused` was hard-coded `false`.
Fix: control commands `RESTART <serial|ALL>`, `FREEZE`, `THAW`, `FROZEN` (host/src/config_handler.rs, session.rs);
`EncoderSettings.restart_nonce` makes the capture loop kill and respawn ffmpeg and evdi_helper; Tauri `save_config` /
`save_device_profile` call it when stream arguments changed; `restart_session`, `pause_device` (SIGSTOP), `resume_device` (SIGCONT) commands.
The old `PAUSE`/`RESUME` (kill, for APK updates) are unchanged.
Proof (GUI encoder dropdown + Save): ffmpeg 442708 -> 443680, evdi_helper 442703 -> 443674, `-c:v hevc_vaapi` -> `h264_vaapi` (`46/ps_restart_proof.png`).
Pause: process state `SNl/SNLl` -> `TNl/TNLl`, stable 8 s, back to running after THAW.

### Parts 3 and 4: onboarding

No Tailscale code exists in the GUI or host (only mentioned in doc 26), so nothing to remove. Wizard now opens on a
"Setup via USB (Recommended)" button with a collapsed "Don't have a USB cable?" section (QR Code / Manual Pairing tabs,
APK note). QR discovery has a hard 30 s limit and emits `mdns_timeout`; the UI shows the warning and switches to Manual Pairing
(`wizard_mdns_timeout.png`). Manual Pairing needs pairing IP:Port, 6-digit code and Connect IP:Port. Wireless success jumps to Finish via the new `wireless_device_info` command.
Not tested: a real QR or manual pairing with a tablet, and the USB install steps (the tablet was already set up).

### Follow-up: USB wireless bootstrap, restart tunnels, Reinstall queue

- `RESTART` now also re-asserts `adb reverse` for the restarted slot (graceful SIGTERM, wait, SIGKILL fallback was already in `terminate()`).
- Re-verified after redeploy with two devices attached: pause froze only the tablet's pipeline (`TNl/TNLl`, the phone's stayed `S`); a GUI encoder change respawned both pipelines with new PIDs (`46/ps_encoder_change.png`).
- **USB bootstrap.** The requested design (Android app reports IP, pairing port and 6-digit code over the USB socket, host runs `adb pair`) is not
  possible: Android exposes no API for the pairing code or port, and they exist only while the system pairing dialog is open. It is also unnecessary, because over USB the
  host already has an authorized adb shell. `bootstrap_wireless_over_usb` (src-tauri/src/pairing.rs) enables `adb_wifi_enabled`, reads the wlan0 IP, connects to the mDNS TLS port,
  and falls back to `adb tcpip 5555` + `adb connect` when mDNS is blocked. The wizard runs it after the install, shows "Configuring wireless mode over USB...", then skips to Finish with the address filled in.
  Verified on the tablet: connected at 192.168.1.101:5555 (tcpip fallback, `46/wizard_finish_autofilled.png`). No Android change was made.
- **Reinstall UI.** Installs run through a single promise chain in App.tsx; the active device shows a spinner and "Reinstalling...", the others "Waiting..." (`46/reinstall_spinner_queue.png`).


## Result

not recorded as a separate section; see What was tested.

## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/46_mega_ui_backend_fixes.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/46_mega_ui_backend_fixes.md)
