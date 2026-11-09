---
title: "uscreen start"
sidebar:
  order: 1
---

`uscreen start` runs the daemon in the foreground until you stop it. With `-d` / `--daemon` it starts in the background and prints the process id. Running `uscreen` with no command does the same as `uscreen start`. The options before the command (`--encoder`, `--fps`, `--bitrate`, `--width`, `--height`, `--quality`, `--stream-scale`, `--pen-only`, `--max-performance`, ports and the helper path) override `config.toml` for this run only.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-start/).

## Sources

- `host/src/main.rs` (the `Start` arm), `host/src/cli/mod.rs`
