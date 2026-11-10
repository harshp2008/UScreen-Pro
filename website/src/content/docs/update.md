---
title: "Update"
---

Update both halves together. The daemon and the app exchange a session token that was introduced in 1.1.0, so an app from a different generation than the daemon does not authenticate.

## Updating the Linux side

Run the installer again from the new checkout or tarball. It replaces the binaries and the Shell extension, and it asks for a restart only when something needs one: a log out and in when the extension version changed on GNOME with Wayland, a reboot when the EVDI module is in use. Installing the same version again asks for nothing.

## Updating the app

When the daemon sees a tablet whose app version differs from its own, it queues an install of the bundled APK over adb (at most two at a time) and shows an "Updating" overlay on the tablet. After a failed automatic update it does not retry for ten minutes, so a stale APK on disk cannot cause a loop. You can also install by hand from the desktop app.

## Update notices

Once a day the daemon asks GitHub for the latest release tag and shows a notice in the tray and in `uscreen doctor`. It never downloads or installs anything. `check_updates = false` in `config.toml` turns the request off. The address it asks is built into the daemon; as of this writing it is the upstream project's release page, not this repository's, so the notice does not track this project's releases.

## Sources

- `host/src/net/installer.rs` (the install queue, the ten-minute cooldown)
- `host/src/daemon/update.rs` (the update check and its address)
- `docs/installer.md` ("When is a restart needed?")
