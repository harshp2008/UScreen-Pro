---
title: "uscreen stop"
sidebar:
  order: 2
---

`uscreen stop` sends SIGTERM to the process named in the process-id file, removes that file, and falls back to sending SIGTERM to any other uscreen daemon process of your user. The daemon shuts down cleanly when it receives the signal.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-stop/).

## Sources

- `host/src/cli/commands.rs` (`stop_daemon`)
