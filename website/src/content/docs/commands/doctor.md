---
title: "uscreen doctor"
sidebar:
  order: 9
---

`uscreen doctor` checks kernel modules, `/dev/uinput` permissions, tools, leftover processes, the tablet over adb, the display mode, the tablet's codec support, colour settings and the config file, and prints a fix for each problem. `uscreen doctor --latency` instead sweeps a synthetic mouse across both screens for `--secs` seconds (10 by default) and reports frame-path latency percentiles; `--sparse` makes it one small step every 100 ms.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-doctor/).

## Sources

- `host/src/cli/doctor/`
