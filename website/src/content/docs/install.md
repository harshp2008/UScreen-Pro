---
title: "Install"
---

UScreen has two halves that ship together and must be updated together: the Linux side (daemon, desktop app, tray) and the Android app. Since version 1.1.0 they share a session token, so one without the other does not connect.

## The Linux side

From a source checkout or an unpacked release tarball, run the installer:

```bash
./scripts/install.sh
```

In a terminal with `whiptail` or `dialog` it asks which parts to install; otherwise it installs everything. To see what it would do without changing anything, add `--dry-run`. To see what your system needs, run `./scripts/install.sh --plan`. All options are on [Installer flags](/UScreen-Pro/installer-flags/).

The installer detects the distribution family and installs the packages that family needs (`ffmpeg`, `adb`, the EVDI virtual-display module, a GTK 4 runtime for the fallback pen menu) with its own package manager. It needs root or `sudo` for that part, and installs everything else into your home directory.

The release packages (`.deb`, `.rpm`, an Arch `PKGBUILD`) and the build scripts exist in `packaging/`. This project has **not run them**: only the installer script, and only on Ubuntu 24.04, has been run on a real system.

## The Android app

Install `uscreen.apk` on the tablet and turn on USB debugging (Settings, About, tap *Build number* seven times, then Developer options, USB debugging). The first time you plug in, accept the "Allow USB debugging" prompt on the tablet. The minimum Android version is 8.1.

## After the install

The result is printed as one line and saved to `~/.local/share/uscreen/install-result.json`:

```text
UScreen-INSTALL-RESULT: relogin=yes|no reboot=yes|no
```

On GNOME with Wayland a new or changed Shell extension loads only after you log out and in once (`relogin=yes`). A reboot is asked for only when the EVDI module is in use and cannot be reloaded, or when packages were layered on an image-based system.

Then start the daemon (`systemctl --user enable --now uscreen`, or `uscreen start`, or the Start button in the desktop app) and follow the [Quick start](/UScreen-Pro/quick-start/).

## Not verified

The package files for Debian, Fedora, openSUSE and Arch, a real EVDI module build on a non-Ubuntu system, and every desktop other than GNOME on Wayland.

## Sources

- `scripts/install.sh` and `scripts/lib/` (what the installer does)
- `docs/installation.md` (the long form, with the packages)
- `SECURITY.md` ("What the installer and packages change on the system")
