---
title: "Findings"
---

What was found, in the maintainer's own working notes, and what each result cannot tell you. The notes are the [experiment records](/UScreen-Pro/experiments/); this page only points at the ones that changed how the program is built. Everything was run on **one machine** (Ubuntu 24.04, GNOME 46, Wayland, an Intel laptop and a Samsung Galaxy Tab S9 FE) unless a record says otherwise. Nothing here was re-run to write this page.

## Four early root causes

Records [1](/UScreen-Pro/experiments/1-bug-1-gui-missing-encoder-option/) to [4](/UScreen-Pro/experiments/4-bug-4-stubborn-meta-0-metavendor-monitor/) trace four problems to their causes.

| Problem | Cause as the notes state it |
| :--- | :--- |
| the HEVC VA-API encoder was missing from the old desktop app's list | the list was hard-coded and left the option out |
| the whole system froze when the tablet was plugged in | module load order and a missing EVDI udev rule let a kernel DRM mutex deadlock happen during hotplug |
| lag and a ghost cursor when the mouse moves from the tablet to the computer | a 250 ms watchdog in the EVDI helper forced grabs without an update-ready event; idle screens starved the FIFO; a software cursor stayed burned into the frame |
| a phantom 1024x600 monitor that survived reboots | a systemd override that passed `--virtual-monitor 1024x600` to GNOME Shell |

## The login freeze

[Record 56](/UScreen-Pro/experiments/56-rca-login-freeze-of-gnome-mutter-46-2-with-the-uscreen-daemon-evdi/) analyses a GNOME login freeze with the daemon and EVDI. Its status line says the fix was implemented and tested in unit and integration tests, that live verification on real EVDI hardware was not yet done, and that the root cause is supported by evidence but not proven by a captured trace. The fix is a settle gate and a hotplug limiter (the "guard" in the daemon).

## The pen on the cursor

Records [100](/UScreen-Pro/experiments/100-resting-pen-no-longer-takes-the-cursor-hover-park-filter-pen-recorder/) to [103](/UScreen-Pro/experiments/103-capturing-the-real-scanout-to-see-the-ghost-cursor-dropped/):

- A resting pen keeps sending hover samples and took the cursor from the mouse; a stillness filter now parks it after 300 ms inside 1.5 mm. The notes say a real S Pen's resting noise is unknown.
- A tap with a little wobble started a tiny drag; the first 2 mm of a stroke on the mouse route now write nothing. On six taps with about 1 mm of wobble the notes record 6 of 6 drags before and 0 of 6 after.
- The pen menu and the desktop layer were built together (record 102).
- Capturing the real scanout with `kmsgrab` did not give a readable picture, and the second attempt was followed by a compositor stall that needed a forced restart. The notes could not separate that from the known EVDI stall of record 56. The method was dropped; the ghost cursor was fixed by snapping the pointer to whole physical pixels.

## Numbers

Latency figures (about 22 ms end to end over USB with H.264 on the upstream reference hardware) and the Wi-Fi comparison are on the Benchmarks page under Project documents. They are the upstream project's measurements and were not repeated on this project's hardware.

## Not verified

The records name their own gaps. In general: real hardware other than the maintainer's, other desktops, other tablets, and long runs.

## Sources

- `experiments_and_findings/INVESTIGATION_SUMMARY.md` and records 01 to 04
- `experiments_and_findings/56_evdi_mutter_freeze_rca.md`
- `experiments_and_findings/100_hover_park_and_recorder.md`, `101_click_slop.md`, `102_pen_menu_and_desktop_layer.md`, `103_scanout_capture_dropped.md`, `104_screenshots_slow_with_virtual_displays.md`
- `docs/benchmarks.md`
