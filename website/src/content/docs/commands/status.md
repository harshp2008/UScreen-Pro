---
title: "uscreen status"
sidebar:
  order: 3
---

`uscreen status` prints `uscreen is running (PID: ..., version: ...)` when the process in the process-id file exists, or `uscreen is not running` (and removes a stale process-id file).

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-status/).

## Sources

- `host/src/cli/commands.rs` (`show_status`)
