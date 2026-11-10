---
title: "Settings and protocol"
---

## Where settings live and who writes them

`~/.config/uscreen/config.toml` is the one store. Three programs change it: the daemon (for settings a tablet pushes), the desktop app (`uscreen-pro-gui`) and, by hand, you. The tablet app does not write the file; it sends a `setting` message and the daemon stores it as that device's override, then answers with a `config_update` that the app follows.

Each setting can exist twice: a global value and a per-device override in `[tablets.<serial>]`. The newest change wins:

- **A global change clears the override** of that setting on every device and keeps the other overrides. The desktop app saves the global settings from its open window, but the profiles on disk are kept, not the window's older copy.
- **A device change** from the tablet or the device page sets the override for that device only.
- **The desktop window follows the file.** It re-reads `config.toml` every 1.5 seconds and merges the new file into the window three ways (what it loaded, what you edited, what is on disk): a value you did not touch takes the new file value, a value you edited stays yours until you save.
- **Unknown keys are kept** when the desktop app saves, so hand-added keys round-trip.

The full key list is on [Configuration](/UScreen-Pro/configuration/).

## Wire protocol

Two loopback connections per tablet slot, reached through `adb reverse`. Ports are 8890 and 8891 for slot 0 and two more for each further slot.

### Video (TCP)

The client first sends the 64-character hex session token. The server then sends packets of `u32 length` (big endian), `u8 type` and a payload. Type 0 is the codec configuration (SPS and PPS, plus VPS for HEVC). Type 1 is a frame: a `u32` sequence number, then the Annex B data. Type 2 is sent once when the daemon exits cleanly, with the payload `{"type":"host_shutdown"}`.

### Input (WebSocket, JSON)

The first message must be `{"type":"auth","token":"..."}`. The server replies with a greeting (`status`, the stream size, `codec`, `pen_only`) and repeats it when the mode changes. Messages from the tablet:

| `type` | Meaning |
| :--- | :--- |
| `touch`, `pen` | position (0 to 1), pressure, action; pen adds tilt in degrees and `eraser`. Actions: 0 down, 1 up, 2 move, 3 hover, 4 hover exit, 5 and 6 stylus button down and up |
| `resolution` | the panel size in pixels and millimetres |
| `config`, `setting` | encoder settings pushed from the tablet; one setting changed on the tablet |
| `mode` | an old request to switch between second screen and graphics tablet; still accepted for older apps |
| `rel`, `button`, `scroll` | Touchpad mode: movement in tablet pixels, a button (0 left, 1 right, 2 middle), a scroll |
| `touchpad_raw` | Touchpad mode with native gestures: every finger that is down, raw |
| `gesture` | `pinch`, `workspace_prev`, `workspace_next` |
| `rendered`, `wake`, `ping` | the frame counter and decode time, a wake after dimming, a keep-alive |

Messages to the tablet are `config_update` (the effective settings) and the notices `paused`, `resumed`, `settings_updating`, `settings_applied`, `updating`, `update_done` and `unpaired`.

Input messages carry a sequence number (`seq`). The daemon drops a repeated DOWN, and an UP or DOWN that arrives older than a newer one, and never drops moves.

## Not verified

The list of message types was read from the parser, not from a capture. Old Android apps are meant to keep working with the new host; no test in the repository runs an old app.

## Sources

- `host/src/input/protocol/mod.rs` and `seq_filter.rs`, `host/src/net/stream.rs`, `host/src/net/notices.rs`
- `docs/architecture.md` ("Protocol")
- `uscreen-pro-gui/src/App.tsx` (`mergeConfig`), `uscreen-pro-gui/src-tauri/src/config.rs`
- `android/app/src/main/java/com/uscreen/HostLink.kt`
