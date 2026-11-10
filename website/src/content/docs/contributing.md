---
title: "Architecture and contributing"
---

## Architecture in one paragraph

The daemon (Rust, `host/`) finds the tablet over adb, starts the EVDI helper (C, `host/evdi/`) that grabs the virtual display, converts it to NV12 and writes it into a pipe, runs ffmpeg on it, and serves the encoded packets on a loopback socket. The Android app (Kotlin) decodes them and sends touch and pen events back over a loopback WebSocket; the daemon turns them into virtual input devices. The desktop app (Tauri and React, `uscreen-pro-gui/`) edits `config.toml` and drives the daemon. Per-desktop code is in `host/src/desktop/<name>/` and the GNOME Shell extension is in `extensions/uscreen-app-lock/`. See [Code layout](/UScreen-Pro/project/code-layout/) for the tree and [Behaviour notes](/UScreen-Pro/project/behavior-notes/) for the pipeline.

## Adding support

A new distribution, desktop or GNOME Shell version is one small change each: see [Adding a distro, a desktop or a GNOME version](/UScreen-Pro/project/adding-support/).

## How to contribute

1. Build and test (see [Testing](/UScreen-Pro/testing/)).
2. Keep a change to one topic. A pure refactor changes no behaviour and says how that was checked.
3. Follow the [comment style](/UScreen-Pro/project/comment-style/): say why, give units, no noise.
4. Describe what you ran and what you could not run. Do not claim a desktop or a tablet works without a run.

## Sources

- `CONTRIBUTING.md`, `docs/development.md`, `docs/architecture.md`, `docs/adding-support.md`
