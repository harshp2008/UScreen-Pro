---
title: "uscreen list-displays"
sidebar:
  order: 4
---

`uscreen list-displays` prints the output of `kscreen-doctor -o` and of `wpctl status` (under the heading "PipeWire"), when those programs exist. It is a KDE-oriented helper: on other desktops it prints little or nothing.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-list-displays/).

## Sources

- `host/src/cli/commands.rs` (`list_displays`)
