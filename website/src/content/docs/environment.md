---
title: "Environment and exit codes"
---

## Environment variables

The daemon and the installer read these. None is needed for normal use.

| Variable | Read by | Meaning |
| :--- | :--- | :--- |
| `XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE` | daemon, installer | decide the desktop backend (`gnome`, `x11`, `generic`) and whether a log out is needed |
| `XDG_RUNTIME_DIR` | daemon, desktop app | where the token, control socket and capture FIFO live (`$XDG_RUNTIME_DIR/uscreen/`); falls back to `/run/user/<uid>` and then `~/.cache` |
| `XDG_STATE_HOME` | daemon | where `guard.json` lives |
| `HOME`, `PATH` | daemon | config and data paths; finding `adb`, `ffmpeg`, `spawn-at` |
| `WAYLAND_DISPLAY`, `DISPLAY`, `HYPRLAND_INSTANCE_SIGNATURE` | daemon | session detection |
| `USCREEN_FAKE_TABLET` | daemon | a serial the daemon treats as an attached tablet; for tests with `scripts/fake-tablet.py` |
| `USCREEN_MOCK_HOST_VERSION` | daemon | pretend to be another version, to test the app-update path |
| `USCREEN_CURSOR_EVENTS` | EVDI helper | `1` leaves the cursor out of the stream (cursor events on); default composites it into the frames |
| `USCREEN_FLUSH_DUP` | EVDI helper | `0` turns off the duplicate write that pushes an isolated frame through the encoder |
| `USCREEN_BOUNDARY_NUDGE` | EVDI helper | `0` disables the X11 boundary-sync window |
| `RUST_LOG` | daemon | log filter, for example `RUST_LOG=uscreen=debug` |
| `US_OS_RELEASE`, `UNAME_R`, `US_OSTREE`, `US_UID` | installer | override the detected system; used by the installer tests |

## Exit codes

| Program | Code | Meaning |
| :--- | :--- | :--- |
| `uscreen` | 0 | the command succeeded |
| `uscreen` | non-zero | a command returned an error (for example `tablet-rules` without a tablet, `debug-dump` without a running daemon, `reset-guard` that cannot delete the record); the message is printed |
| `scripts/install.sh` | 0 / 2 / 64 | done / needs root / unknown option (see [Installer flags](/UScreen-Pro/installer-flags/)) |
| `scripts/tests/run.sh` | 0 / 1 | all tests passed / at least one failed |

## Not verified

The exact non-zero value of `uscreen` errors was not checked; it comes from the error being returned from `main`.

## Sources

- `host/src/main.rs`, `host/src/daemon/runtime.rs`, `host/src/desktop/mod.rs`, `host/src/daemon/guard/boot_loop.rs`
- `host/evdi/helper_events.h`, `helper_state.h`, `evdi_helper.c` (the helper's variables)
- `scripts/lib/platform.sh`, `scripts/install.sh`, `scripts/tests/run.sh`
