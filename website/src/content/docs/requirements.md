---
title: "Requirements"
---

## The computer

| Needs | Why | Without it |
| :--- | :--- | :--- |
| Linux with the EVDI kernel module | creates the virtual display | Extended Display mode does not work; Stylus and Touchpad modes do not need it |
| `adb` | talks to the tablet over USB and starts the app | nothing connects |
| `ffmpeg` with an encoder that works (`h264_nvenc`, `hevc_nvenc`, `h264_vaapi`, `hevc_vaapi` or `libx264`) | encodes the picture | no picture |
| `/dev/uinput` writable by your user | creates the virtual pen, touch and pointer devices | no input reaches the desktop |
| a GTK 4 runtime | the fallback pen menu window | the menu is drawn by the GNOME extension instead, or not at all |
| GNOME Shell 45 or later (optional) | the extension reports the focused app and draws the pen menu | the pen cannot switch style per app |

`uscreen check-encoder <name>` tells you whether an encoder can encode a test frame, and `uscreen doctor` reports what is missing on your system.

## The tablet

Android 8.1 or newer, with a hardware H.264 decoder, and a data-capable USB cable. USB debugging must be on. Wi-Fi works as a fallback, with more stutter than the cable.

## The desktop

Only Ubuntu 24.04 with GNOME 46 on Wayland has been run. See [Environments](/UScreen-Pro/environments/).

## Sources

- `README.md` ("Quick install", "FAQ")
- `docs/installation.md` ("Dependencies by distribution")
- `host/src/capture/encoders.rs` (the encoder list)
- `host/src/cli/doctor/` (what `uscreen doctor` checks)
