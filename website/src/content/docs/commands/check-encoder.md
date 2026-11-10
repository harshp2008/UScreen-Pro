---
title: "uscreen check-encoder"
sidebar:
  order: 8
---

`uscreen check-encoder <encoder>` encodes one test frame with that encoder (`h264_nvenc`, `hevc_nvenc`, `h264_vaapi`, `hevc_vaapi`, `libx264`) and says whether it worked, without starting the daemon. `--vaapi-device <node>` picks the render node; an empty value picks one automatically.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-check-encoder/).

## Sources

- `host/src/capture/encoders.rs`
