---
title: "Known limits"
---

Numbers the program enforces. A value outside a range is clamped when `config.toml` is loaded.

| What | Limit | Where |
| :--- | :--- | :--- |
| `bitrate` | 1 000 to 60 000 kbit/s | `limits.rs` |
| `fps` | 10 to 90 | `limits.rs` |
| `quality` (constant-quality target; lower is sharper) | 12 to 32, default 18 | `limits.rs` |
| idle dim timeout | 10 to 300 s | `limits.rs` |
| dim fade time | 100 to 30 000 ms | `limits.rs` |
| custom fade curve control points | x within 0 to 1, y within -2 to 3 | `curves.rs` |
| `max_tablets` | 1 to 4 | `config/sanitize.rs` |
| `stream_scale` | 1 to 4 (integer downscale of the stream only) | `config/sanitize.rs`, `evdi_helper.c` (`--scale`) |
| the helper's frame rate | 1 to 240 (60 if outside) | `evdi_helper.c` (`--fps`) |
| touch fingers in Touchpad mode | up to 10 contacts | `uinput/pad.rs` |
| install concurrency | 2 installs at a time | `net/installer.rs` |
| NV12 conversion threads | 1 to 4 | `helper_convert.h` |
| tablet app | Android 8.1 or newer | `README.md` |

## Sources

- `host/src/config/limits.rs`, `host/src/config/curves.rs`, `host/src/config/mod.rs`
- `host/evdi/evdi_helper.c`, `host/evdi/helper_convert.h`
- `host/src/input/uinput/pad.rs`, `host/src/net/installer.rs`
