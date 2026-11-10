<!--
  README.md: the project's front page: what UScreen Pro is, how to install it, and what has been run.
  Owns the short overview; the complete feature table is docs/features.md, the details are on the documentation site (website/).
-->
<p align="center">
  <img src="docs/assets/screenshots/dark-settings-modes.png" alt="The UScreen Pro desktop app (sample data)" width="820">
</p>

<h1 align="center">UScreen Pro</h1>

<p align="center">
  Your Android tablet as a <b>second display</b>, a <b>laptop touchpad</b> and a <b>real pen tablet</b>, on Linux.
</p>

<p align="center">
  <a href="https://harshp2008.github.io/UScreen-Pro/"><b>Docs</b></a> ·
  <a href="#install">Install</a> ·
  <a href="docs/features.md">All features</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

<p align="center">
  <a href="https://github.com/harshp2008/UScreen-Pro/actions/workflows/build.yml"><img alt="Build" src="https://github.com/harshp2008/UScreen-Pro/actions/workflows/build.yml/badge.svg"></a>
  <a href="https://github.com/harshp2008/UScreen-Pro/actions/workflows/installer.yml"><img alt="Installer tests" src="https://github.com/harshp2008/UScreen-Pro/actions/workflows/installer.yml/badge.svg"></a>
  <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue">
</p>

## What Pro adds

Pro is an extension of [UScreen](https://github.com/majmichu1/UScreen) by majmichu1, built on top of their work to make it comfortable to use every day.

- ✏️ **A pen that works.** A pressure tablet in drawing apps and a normal mouse everywhere else, switched per app (Dynamic: GNOME, experimental) or set by hand.
- 📋 **Cut, copy and paste from the pen.** Hold it still and a small menu opens at the tip.
- 🖱️ **A real touchpad.** Native swipes and pinch, palm rejection, right-click zone.
- ⚡ **Fast and smooth.** Tuned HEVC, steady frames, no ghost cursor.
- 🔋 **Battery care.** Dim when idle, battery saver when the computer is gone.
- 📶 **Wired or wireless.** USB and Wi-Fi, with a setup wizard. Tailscale is experimental.
- 🔄 **Self-updating.** The tablet app is reinstalled when it stops matching the computer.
- 🛟 **Helpful errors.** Each failure says what to do and has a button for it.
- 🧰 **New apps.** A desktop app with a device wizard and per-device settings, and a redesigned tablet app.

Every feature, with where it lives and whether it was run, is in [`docs/features.md`](docs/features.md).

<p align="center">
  <img src="docs/assets/screenshots/tour.gif" alt="A tour of the desktop app (sample data)" width="700">
</p>

## Install

```bash
./scripts/install.sh        # add --dry-run to see every command first
```

Then install `uscreen.apk` on the tablet, plug it in and run `uscreen start`. If something is off, run `uscreen doctor`. Details: [Install](https://harshp2008.github.io/UScreen-Pro/install/).

## Status

Run so far on **Ubuntu 24.04, GNOME 46, Wayland** with a Galaxy Tab S9 FE. **Not verified:** other desktops, distributions and tablets, pen pressure inside a drawing app such as Krita (checked at the input-device level only), QR pairing, Tailscale, and the release packages. The screenshots are the app's sample data, not a connected tablet.

## Credits

UScreen Pro is an extension of **[UScreen](https://github.com/majmichu1/UScreen) by [majmichu1](https://github.com/majmichu1)**, and a thank-you to it: the virtual display, the capture pipeline, the adb connection, the session token and the first Android app are theirs. Pro would not exist without that work. MIT licence, see [`LICENSE`](LICENSE) and [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md). Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md).
