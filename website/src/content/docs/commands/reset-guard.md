---
title: "uscreen reset-guard"
sidebar:
  order: 13
---

`uscreen reset-guard` deletes the compositor-safety record. After a start that ended in safe mode (no virtual display), run it and restart the daemon to allow the virtual display again. It says when there was nothing to clear.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-reset-guard/).

## Sources

- `host/src/daemon/guard/`
