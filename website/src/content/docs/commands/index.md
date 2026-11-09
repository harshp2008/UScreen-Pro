---
title: "Commands"
sidebar:
  order: 0
  label: "Overview"
---

One page for each command of `uscreen`. The exact options of each are on the [CLI reference](/UScreen-Pro/reference/), taken from the program itself.

| Command | What it does |
| :--- | :--- |
| [`uscreen start`](/UScreen-Pro/commands/start/) | Start the daemon |
| [`uscreen stop`](/UScreen-Pro/commands/stop/) | Stop the daemon |
| [`uscreen status`](/UScreen-Pro/commands/status/) | Show the daemon status |
| [`uscreen list-displays`](/UScreen-Pro/commands/list-displays/) | List displays |
| [`uscreen wifi`](/UScreen-Pro/commands/wifi/) | Switch the tablet to Wi-Fi |
| [`uscreen pause-device`](/UScreen-Pro/commands/pause-device/) | Pause one device |
| [`uscreen resume-device`](/UScreen-Pro/commands/resume-device/) | Resume one device |
| [`uscreen check-encoder`](/UScreen-Pro/commands/check-encoder/) | Check an encoder |
| [`uscreen doctor`](/UScreen-Pro/commands/doctor/) | Diagnose the setup |
| [`uscreen tablet-rules`](/UScreen-Pro/commands/tablet-rules/) | Print udev rules for a physical tablet |
| [`uscreen toggle-tablet`](/UScreen-Pro/commands/toggle-tablet/) | Flip the pen route |
| [`uscreen debug-dump`](/UScreen-Pro/commands/debug-dump/) | Write the pen recorder |
| [`uscreen reset-guard`](/UScreen-Pro/commands/reset-guard/) | Clear the safety record |

No command is the same as `start`.

## Sources

- `host/src/cli/mod.rs`
