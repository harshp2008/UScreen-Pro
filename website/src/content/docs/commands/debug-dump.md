---
title: "uscreen debug-dump"
sidebar:
  order: 12
---

`uscreen debug-dump` asks the running daemon to write the pen recorder (the last few thousand pen samples and what the router did with each) to `~/.local/share/uscreen/pen_dump.txt`. Run it right after the pen misbehaves. It fails with a message if the daemon does not answer.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-debug-dump/).

## Sources

- `host/src/input/recorder.rs`, `host/src/main.rs`
