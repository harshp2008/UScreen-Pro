---
title: "Quick start"
---

This assumes you have followed [Install](/UScreen-Pro/install/) and are on a supported setup (see [Requirements](/UScreen-Pro/requirements/)).

1. Plug the tablet in with a data cable and accept the USB debugging prompt on the tablet.
2. Start the daemon: `uscreen start` (one session), or `systemctl --user enable --now uscreen` (every login), or the Start button in `uscreen-pro-gui`.
3. The daemon finds the tablet over adb, forwards the ports, launches the app on it and, in Extended Display mode, creates a virtual display sized to the tablet's panel. The tablet appears as a new monitor in your display settings.
4. To change what the tablet does, open the settings on the tablet, or open the device page in `uscreen-pro-gui`. The three modes are on [Concepts](/UScreen-Pro/concepts/).
5. If something does not work, run `uscreen doctor`. It reports what is missing and what looks wrong; [Troubleshooting](/UScreen-Pro/troubleshooting/) explains the common messages.

To use the tablet without a cable afterwards, run `uscreen wifi` once with the cable in; it remembers the address and reconnects by itself when the cable is out.

## Not verified

Everything on this page was written from the documentation and the code. On Ubuntu 24.04 with GNOME 46 on Wayland the daemon, the desktop app and the pen menu were exercised by the maintainer during development; no test in the repository drives a real tablet.

## Sources

- `docs/installation.md` ("Plug in and use")
- `README.md` ("Settings", Wi-Fi)
- `host/src/cli/mod.rs`
