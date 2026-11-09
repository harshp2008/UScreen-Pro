---
title: "47 Device management and real feedback"
sidebar:
  order: 32
  label: "47 Device management and real feedback"
---

Rig: `./build-all.sh`, daemon restarted through `systemctl --user restart uscreen`, GUI launched with `GDK_BACKEND=x11` and driven with `xdotool`, window captured with `import -window`. Tablet screens captured with `adb exec-out screencap`. Two real tablets: Galaxy A55 (PHONE_SERIAL, Wi-Fi at test time) and Tab S9 FE (TABLET_SERIAL, USB). Screenshots and logs: `47/`.

| Field | Value |
| :-- | :-- |
| **ID** | 47 |
| **Date** | not recorded |
| **Build** | `0c4e908-dirty`, `82b9705-dirty` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/47/`; not published. |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### What changed

| Task | Change |
|---|---|
| 1 Sidebar | `ConnectedDevice.paired` (profile exists in `config.toml`). Sidebar shows **New Devices** (connected, not paired) and **Paired Devices** (saved, any transport or offline). A new device opens the Add-device wizard. |
| 2 Forget | `forget_device` (src-tauri/src/forget.rs): removes `[tablets.<serial>]` plus legacy `ip:port` duplicates with the same label, clears a global `wifi_address` that pointed at it, sends `FORGET` to the daemon (stops the slot, clears frozen/pause/notice state), runs `adb disconnect` for every address, strips the device's record from `~/.android/adb_known_hosts.pb`. Card disappears immediately. |
| 3 Restart feedback | Tauri emits `pipeline_restarting` on Save, daemon `RESTART_WAIT` replies when the **new ffmpeg has produced a frame**, Tauri emits `pipeline_restarted`. Card shows a spinner and "Restarting...". Tablet shows "Settings updating..." until then. |
| 4 Pause | `FREEZE`/`THAW` push `{"type":"paused"}` / `{"type":"resumed"}` over the input WebSocket. Tablet overlay "Connection Paused by Host". Sticky: replayed if the tablet reconnects while paused. |
| 5 Auto-update | On every connect of a paired device (USB or Wi-Fi) the daemon compares `dumpsys package com.uscreen` `versionName` with its own `GIT_VERSION_STRING` and queues `adb install -r`. Tablet shows "Updating App... (Host: X, Device: Y)". Card shows "Updating APK...". |
| 6 Queue | `host/src/installer.rs`: one queue, semaphore of 2, states `queued/installing/done/failed` exposed by `INSTALLS`, shown per card and on the Health page. The Reinstall/Update buttons use the same queue (`queue_apk_install`). |
| 7 QR fallback | After the backend's 30 s mDNS timeout the wizard disables the QR tab, switches to Manual Pairing and shows the required banner. Ignored once a USB bootstrap succeeded (the QR session is also cancelled then). |

### Behaviour change to know about

The daemon now streams only **paired** devices (`FileConfig::is_paired`). Before, any device on adb got a session and a profile automatically, which made "New Devices" impossible (it flipped to Paired in 3 s). `auto_pair_new_devices = true` in `config.toml` restores the old behaviour for CLI-only setups. Fake serials (scripts/fake-tablet.py) are treated as paired.

### Results on hardware

**Auto-update (daemon restart, both tablets had `0c4e908-dirty`, host `82b9705-dirty`):**
```
APK check TABLET_SERIAL: device 0c4e908-dirty != host 82b9705-dirty -> queueing adb install -r
Install queue: TABLET_SERIAL started (1/2 slots busy)
APK check PHONE_SERIAL: device 0c4e908-dirty != host 82b9705-dirty -> queueing adb install -r
Install queue: PHONE_SERIAL started (2/2 slots busy)
Install queue: TABLET_SERIAL finished: updated 0c4e908-dirty -> 82b9705-dirty   (18 s)
Install queue: PHONE_SERIAL finished: updated 0c4e908-dirty -> 82b9705-dirty
APK check PHONE_SERIAL: device 82b9705-dirty matches host 82b9705-dirty      (next connect: no reinstall)
```
**Queue with three devices** could not be run on hardware (two tablets). Covered by `installer::tests::at_most_two_installs_run_and_the_third_waits` (`47/host_unit_tests.txt`): at 150 ms two `installing` + one `queued`, at 550 ms the third `installing`, peak concurrency 2.

**Pause:** `FREEZE` → pipeline `TNl/TNLl` (SIGSTOP), tablet overlay `47/device_paused_overlay.png`; `THAW` → resumed.

**Restart:** `RESTART_WAIT PHONE_SERIAL` → `OK {"restarted":[{"instance":1,"live_ms":1124,"serial":"PHONE_SERIAL"}]}`; PIDs 523096/523101 → 523729/523734; tablet overlay `47/device_settings_updating_overlay.png`. GUI: fps 90→60 + Save changed `config.toml` and respawned both pipelines (`47/gui_restarting_state.png` shows "Restarting..." on both cards).

**Forget (Wi-Fi A55)** `47/forget_before.txt` → `47/forget_after.txt`: `192.168.1.102:5555` gone from `adb devices` (toast: "2 adb disconnect(s) run"), `[tablets.PHONE_SERIAL]`, `10.238.4.100:5555`, `192.168.1.102:5555` entries and the global `wifi_address` removed, daemon: `Forgot device PHONE_SERIAL: stopped ["slot 2"]`, its ffmpeg/evdi_helper gone, the other tablet's pipeline untouched.
**Forget (USB S9 FE)** `47/forget_usb_after.txt`: moves to **New Devices** (`47/gui_new_devices_after_usb_forget.png`), `adb_known_hosts.pb` 26 → 0 bytes (its `adb-TABLET_SERIAL-…` record).
**QR fallback** on the real app, where `adb mdns services` is empty on this network: `47/wizard_mdns_blocked.png` after 30 s.

After the tests the config and `adb_known_hosts.pb` were restored from a backup and the Wi-Fi tablet reconnected (`APK check … matches host`).

### Limits, stated plainly

- Install overlay: `adb install -r` replaces the running app, so "Updating App..." is visible during the push/verify, then the process is replaced; it cannot stay up to the end. The new app is relaunched by the watchdog.
- Pairing keys: `~/.android/adbkey` is the host's own key shared by all devices and is kept. Only the per-device record in `adb_known_hosts.pb` is removed; the running adb server keeps its in-memory copy until it restarts (not restarted: it would drop every tunnel). The tablet still lists this computer under Wireless debugging.
- "Live" detection counts frames of the non-`inproc-encoder` path; with that feature the restart is reported after the 25 s timeout instead.
- A stale `[tablets."192.168.1.101:5555"]` (empty label) in this machine's config is not matched to any device, so Forget leaves it.
- Not exercised: the QR/manual pairing flows with a real phone, and the "Reinstall" button path (same queue as auto-update, covered by the log above).


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/47_device_management_and_feedback.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/47_device_management_and_feedback.md)
