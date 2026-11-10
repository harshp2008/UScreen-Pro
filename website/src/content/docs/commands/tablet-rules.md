---
title: "uscreen tablet-rules"
sidebar:
  order: 10
---

`uscreen tablet-rules` prints the udev rules that give you access to a physical drawing tablet's pen without root and, with `--hide`, the rule that hides it from the desktop (`LIBINPUT_IGNORE_DEVICE`). It installs nothing; the output says where each file goes. `--vid` and `--pid` (hex) pick the device; without them the first pen tablet found is used. It fails with a message when none is found.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-tablet-rules/).

## Sources

- `host/src/input/physical.rs` (`tablet_rules`), `packaging/tablet/README.md`
