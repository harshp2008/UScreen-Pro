<!--
  README.md: the project's front page: what UScreen Pro is, how to install it, what was run, and the map of the repository.
  Owns the overview; the details are on the documentation site (website/) and in docs/.
-->
<p align="center">
  <img src="docs/assets/screenshots/dark-settings-modes.png" alt="The UScreen Pro desktop app in the dark theme, showing the mode settings (sample data)" width="860">
</p>

<h1 align="center">UScreen Pro</h1>

<p align="center">
  Your Android tablet as a <b>second display</b>, a <b>laptop touchpad</b> or a <b>pressure-sensitive pen tablet</b>, on Linux.
</p>

<p align="center">
  <a href="https://harshp2008.github.io/UScreen-Pro/"><b>Documentation</b></a> ·
  <a href="https://harshp2008.github.io/UScreen-Pro/install/">Install</a> ·
  <a href="https://harshp2008.github.io/UScreen-Pro/concepts/">Concepts</a> ·
  <a href="https://harshp2008.github.io/UScreen-Pro/experiments/">Experiments</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

<p align="center">
  <a href="https://github.com/harshp2008/UScreen-Pro/actions/workflows/build.yml"><img alt="Build" src="https://github.com/harshp2008/UScreen-Pro/actions/workflows/build.yml/badge.svg"></a>
  <a href="https://github.com/harshp2008/UScreen-Pro/actions/workflows/installer.yml"><img alt="Installer tests" src="https://github.com/harshp2008/UScreen-Pro/actions/workflows/installer.yml/badge.svg"></a>
  <a href="https://github.com/harshp2008/UScreen-Pro/actions/workflows/docs.yml"><img alt="Docs" src="https://github.com/harshp2008/UScreen-Pro/actions/workflows/docs.yml/badge.svg"></a>
  <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue">
</p>

> **What has been run.** The only platform this project has been run on is **Ubuntu 24.04 with GNOME 46 on Wayland**.
> Other desktops, distributions and the release packages are described in the docs as *not verified*. Nothing here claims they work.

## What it does

A small daemon on your computer talks to an app on the tablet over a USB cable (or Wi-Fi). The tablet is in one of three modes:

| | Mode | What you get |
|---|---|---|
| 🖥️ | **Extended Display** | A real second monitor: a virtual display is created, encoded and streamed to the tablet; touch and pen come back. |
| 🖱️ | **Touchpad** | The tablet is a laptop touchpad: move, tap to click, two-finger scroll, tap-and-drag, three-finger gestures, palm rejection. Nothing is streamed. |
| ✏️ | **Stylus** | A pen tablet for the screen you are already looking at: pressure, tilt, eraser and side buttons, with no display latency. |

For the pen there are three styles: **Mouse**, **Graphics tablet** and **Dynamic** (experimental, GNOME only), which acts as a pressure tablet in drawing apps and as a mouse everywhere else. A long press opens a small **pen menu** (cut, copy, paste, select all, undo) where it makes sense.

## See it

<p align="center">
  <img src="docs/assets/screenshots/tour.gif" alt="A tour of the desktop app: mode settings, a device page, system health and the add-device wizard (sample data)" width="720">
</p>

<table>
  <tr>
    <td><img src="docs/assets/screenshots/dark-device.png" alt="A device page: identity, transport and per-device overrides (dark)"></td>
    <td><img src="docs/assets/screenshots/dark-health.png" alt="System health: daemon, build and device tools (dark)"></td>
  </tr>
  <tr>
    <td><img src="docs/assets/screenshots/dark-wizard.png" alt="The add-device wizard (dark)"></td>
    <td><img src="docs/assets/screenshots/light-device.png" alt="The same device page in the light theme"></td>
  </tr>
</table>

The screenshots show the desktop app with the sample data of its development mode, not a connected tablet. Photos and a recording of a
real tablet are not in the repository yet; [`docs/assets/README.md`](docs/assets/README.md) says where to add them.

## Install

1. **Linux side.** From a checkout or an unpacked release:

   ```bash
   ./scripts/install.sh            # asks what to install; add --dry-run to see every command first
   ```

   It detects your distribution family, installs `ffmpeg`, `adb` and the EVDI virtual-display module with the right package manager,
   and puts everything else in your home directory. It tells you at the end whether you need to log out or reboot
   (`UScreen-INSTALL-RESULT: relogin=… reboot=…`). Options: [`docs/installer.md`](docs/installer.md).
2. **Tablet.** Install `uscreen.apk` and turn on USB debugging.
3. **Plug in** and start the daemon (`uscreen start`, or the Start button in the desktop app). If something is off, run `uscreen doctor`.

Update both halves together: they share a session token. Details and what the installer changes on your system: [Install](https://harshp2008.github.io/UScreen-Pro/install/) and [`SECURITY.md`](SECURITY.md).

## What is new in UScreen Pro

UScreen Pro continues [UScreen by majmichu1](https://github.com/majmichu1/UScreen) (see [Credits](#credits)). What this project added or changed:

- **The Dynamic pen style** with a per-stroke state machine: click slop, resting-pen parking, pointer snapping to whole pixels.
- **The pen menu**, drawn inside GNOME Shell by the extension (no window, no focus change), with a GTK fallback.
- **Settings that follow you:** the desktop app and the tablet app edit the same file, and the newest change wins.
- **A new desktop app** ([Tauri](https://tauri.app/) + React) with a device wizard, per-device overrides and a health page.
- **An installer that is tested:** one file per distribution family, a dry-run and a plan mode, and 60 tests that run in containers of six distributions.
- **A layout built to be extended:** a new distribution is one file, a new desktop is one directory, a new GNOME Shell version is one table row. See [Adding support](https://harshp2008.github.io/UScreen-Pro/project/adding-support/).
- **Every function documented, and the experiments published:** 62 records of what was tried and found, in [Findings and experiments](https://harshp2008.github.io/UScreen-Pro/findings/).

## How it works

```text
 Computer                                                         Tablet
 ┌──────────────┐   frames   ┌───────────┐  H.264 / HEVC   ┌─────────────────────────┐
 │ EVDI virtual │ ─────────▶ │ ffmpeg    │ ───── USB ────▶ │ Android app: decodes    │
 │ display (C   │   NV12     │ encoder   │   (adb reverse) │ and shows the picture   │
 │ helper)      │            └───────────┘                 │                         │
 └──────────────┘            ┌───────────┐  touch, pen     │ sends touch, pen and    │
   Daemon (Rust) ◀────────── │ uinput    │ ◀──── JSON ──── │ touchpad events         │
   + GNOME extension         │ devices   │   (WebSocket)   └─────────────────────────┘
                             └───────────┘
```

Both connections are on loopback and need a per-run session token. The details are in [Behaviour notes](https://harshp2008.github.io/UScreen-Pro/project/behavior-notes/).

## Repository map

| Path | What is in it |
|---|---|
| `host/` | the daemon (`uscreen`, Rust); `host/evdi/` the C helper; `host/src/desktop/<name>/` one directory per desktop |
| `uscreen-pro-gui/` | the desktop app (Tauri + React) |
| `android/` | the tablet app (Kotlin, Jetpack Compose) |
| `menu/` | the GTK 4 fallback pen menu |
| `extensions/uscreen-app-lock/` | the GNOME Shell extension, with a `compat/` layer for Shell versions |
| `scripts/` | the installer (`install.sh`, `lib/distros/<family>.sh`) and its tests |
| `website/` | the documentation site ([Starlight](https://starlight.astro.build/)) |
| `docs/`, `experiments_and_findings/` | the project documents and the original write-ups |

## Contributing

Build and test commands are on [Testing](https://harshp2008.github.io/UScreen-Pro/testing/), the comment rules in [`docs/comment-style.md`](docs/comment-style.md), and the contribution
workflow in [`CONTRIBUTING.md`](CONTRIBUTING.md). Security issues: [`SECURITY.md`](SECURITY.md).

## Credits

UScreen Pro is a continuation of **[UScreen](https://github.com/majmichu1/UScreen) by [majmichu1](https://github.com/majmichu1)**, which provided the original design, the
EVDI capture pipeline and the first Android app. This repository keeps its history and its MIT licence. Third-party libraries are listed in
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md); the window-placement tool [spawn-at](https://github.com/harshp2008/spawn-at) is a companion project.

## License

MIT. See [`LICENSE`](LICENSE).
