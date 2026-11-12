<!--
  docs/architecture/overview.md: how the pieces fit: the three programs, the repository layout, the data flow between them,
  the capture pipeline, the wire protocol and the security model.
  Owns the big picture and the map from a responsibility to the directory that holds it. Per-component detail is in the source
  comments; the platform design is in platform-modularity.md.
-->
# Architecture overview

UScreen turns an Android tablet into a second display and a graphics tablet for a Linux computer. It is three programs that
share one settings file, plus a few helper processes.

| Program | Language | Where | Job |
|---|---|---|---|
| **Host daemon** `uscreen` | Rust | `host/` | Creates the virtual display, captures and encodes it, serves the video and input sockets, injects input, supervises the tablet |
| **Desktop app** `uscreen-pro-gui` | Rust (Tauri 2) + React | `uscreen-pro-gui/` | Settings window, device wizard, daemon start and stop. Never touches video or input |
| **Tablet app** | Kotlin (Compose) | `android/` | Decodes and shows the video; sends touch, pen and touchpad events; applies the settings the host pushes |
| `evdi_helper` | C | `host/evdi/` | One per tablet slot. Owns an EVDI card, converts frames to NV12 and writes them to a FIFO |
| `uscreen-menu` | Rust (GTK 4) | `menu/` | The pen's long-press menu window, started by the daemon |
| Shell extension | JavaScript | `extensions/uscreen-app-lock/` | GNOME Shell only: window lock, pen routing, pointer context, in-shell pen menu |
| `ffmpeg` | external | | One per slot: encodes the raw frames (unless built with `inproc-encoder`) |

## Repository layout

```text
host/src/
├── main.rs, cli/            command line (start, stop, status, doctor, ...)
├── config/                  config.toml: model, defaults, sanitising, the per-device options the tablet receives
├── daemon/                  lifecycle, tray icon, update check
│   ├── session/             one state machine per tablet slot (adb monitor, pipeline, profiles)
│   └── guard/               compositor safety: freeze watchdog, boot-loop breaker (GNOME Wayland only)
├── capture/                 EVDI helper supervisor, encoder, Annex B packetizer, latency log
├── display/                 desktop-independent virtual monitor: generated EDID, EVDI card discovery in sysfs
├── desktop/                 everything that depends on WHICH desktop runs (see platform-modularity.md)
│   ├── mod.rs               DesktopBackend trait: focus, pointer context, pen menu, window placement
│   ├── display.rs           display dispatcher: enable / disable the virtual output
│   ├── scale.rs             interface-scale dispatcher
│   └── gnome/ x11/ kde/ hyprland/ cosmic/ generic/    one directory per desktop
├── input/                   WebSocket input server and everything between it and uinput
│   ├── protocol/, events/   decoded messages and their handlers
│   ├── ghost/               pen routing between the tablet device and the pointer device
│   ├── menu/                long-press menu decisions and the runner of uscreen-menu
│   ├── mapping.rs           pin the virtual devices to the tablet's output (per desktop)
│   ├── physical.rs          pass a real USB tablet through
│   └── uinput/              virtual device creation and evdev writes (Linux)
└── net/                     adb (discovery, reverse tunnels, wireless pairing, app launch), video stream server, APK installer

uscreen-pro-gui/src/         React UI
├── App.tsx                  shared state and view routing
├── views/                   Settings, Device settings, Advanced, Health
├── components/
│   ├── shell/               sidebar, title bar
│   ├── wizard/              Add Device wizard and manual pairing
│   ├── power/               the settings sections shared by the global and the device page, curve editor
│   └── ui/                  design-system building blocks (buttons, rows, controls)
└── lib/                     types, options, config merge, scroll memory, theme, toast, dev mock
uscreen-pro-gui/src-tauri/   Tauri commands: adb, pairing, daemon control, config.toml, forget

android/app/src/main/java/com/uscreen/
├── MainActivity.kt, StreamingService.kt, receivers    entry points (named in the manifest, so they stay in the root package)
├── HostLink.kt, Prefs.kt                              what the host pushes; what the tablet remembers
├── PlatformSupport.kt, PowerSupport.kt, OrientationPolicy.kt    Android-version-specific helpers
├── input/               TouchCapture (socket + pen + touchpad feed), TouchpadEngine, tap logic, palm filter, pen tilt, haptics
├── video/               VideoReceiver (socket + MediaCodec)
└── ui/                  Compose screens
```

## Data flow

```text
                        config.toml  (~/.config/uscreen)
                  ┌──────────┴───────────┐
                  │ read / write         │ read / write
        ┌─────────┴────────┐    control socket, pid file, systemctl
        │   Desktop app    │───────────────────────────────┐
        └──────────────────┘                               │
                                                           ▼
 compositor ◄── enable / place / map ────────────┐  ┌────────────────┐
   │  renders into the virtual monitor           └──┤  Host daemon   ├── ffmpeg (encode)
   ▼                                                 │   `uscreen`    │       ▲
 evdi_helper ── NV12 frames over a FIFO ─────────────►                ├───────┘
                                                     └──┬──────────┬──┘
                       Annex B access units over TCP    │          │  JSON over WebSocket
                       (loopback, `adb reverse`)        ▼          ▲
                                                 ┌──────────────────┴──┐
                                                 │      Tablet app     │
                                                 │ MediaCodec → screen │
                                                 │ touch / pen / pad ──┘
                                                 └─────────────────────┘
```

* **Video** goes host → tablet only: compositor → `evdi_helper` → FIFO → `ffmpeg` → packetizer → TCP server → tablet decoder.
* **Input** goes tablet → host: touch, pen and touchpad events → WebSocket → `input/events` → uinput devices → compositor.
* **Settings** meet in `config.toml`. The desktop app writes it and re-reads it every 1.5 s so a change made on the tablet
  appears in the open window (`lib/configMerge.ts` merges the two without losing edits). The host reads it, sanitises it
  (`config/sanitize.rs`) and pushes each tablet its effective settings (`config/options/`) over the WebSocket; a setting
  changed on the tablet comes back the same way and is written to the device's `[tablets.<id>]` section.
* **Control** from the desktop app to the daemon uses `systemctl --user` to start and stop the unit, plus the Unix control socket
  served by `config/handler.rs` for requests from the app and the command line.
* **Desktop-specific work** (placing the virtual output, pinning input devices to it, focus, the pen menu) is requested through
  `desktop/`; no other module asks which desktop it is.

## Where each responsibility lives

| Responsibility | Host | Desktop app | Tablet app |
|---|---|---|---|
| Settings model and defaults | `config/` | `lib/types.ts`, `lib/options.ts`, `src-tauri/src/config.rs` | `HostLink.kt` (`HostConfig`), `Prefs.kt` |
| Settings parity check | `scripts/tests/settings_parity.py` reads all three | | |
| Pairing and adb | `net/adb/`, `net/pairing.rs` | `src-tauri/src/adb/`, `pairing.rs`, wizard | (none) |
| App installation | `net/installer.rs` | `src-tauri/src/adb/install.rs` | |
| Virtual display | `display/`, `capture/`, `desktop/display.rs` | (none) | |
| Video | `capture/`, `net/stream.rs` | (none) | `video/VideoReceiver.kt` |
| Input injection | `input/` | (none) | `input/` (produces the events) |
| Pen routing, long-press menu | `input/ghost/`, `input/menu/`, `desktop/` | settings only | `input/TouchCapture.kt` |
| Compositor safety | `daemon/guard/` | (none) | |
| Power and idle dimming | `config/options/idle.rs` (the idle state machine; sends dim and undim notices) | settings only | `PowerSupport.kt` (fades the window brightness) |
| Updates | `daemon/update.rs` (announces), `net/installer.rs` (installs the APK) | | |

## Pipeline

1. **Virtual display.** The daemon starts a small C helper that opens an EVDI device (`/dev/dri/cardN` provided by the `evdi`
   kernel module) and presents a generated EDID with the tablet's resolution and physical size. The compositor sees a new
   monitor and starts rendering to it; the daemon enables it and places it next to the real screens through the display
   dispatcher (`desktop/display.rs`): `kscreen-doctor` on KDE, `hyprctl` on Hyprland, `xrandr` on X11, and nothing on GNOME,
   where Mutter does it itself.
2. **Capture.** The helper runs an event-driven `request_update` / `grab_pixels` cycle at the target frame rate, converts BGRA
   to NV12 (BT.709, limited range) and writes whole frames into a FIFO in the per-user runtime directory. Only rows the
   compositor reported as damaged are converted.
3. **Encode.** `ffmpeg` (or libavcodec in-process) encodes with NVENC, VAAPI or libx264 in constant-quality mode, no B-frames,
   no lookahead, one keyframe per second plus keyframes on demand when a client joins.
4. **Stream.** A TCP server on loopback sends length-prefixed Annex B access units; `adb reverse` carries the port to the
   tablet over USB. A client that falls behind is skipped forward to the newest keyframe rather than fed a backlog.
5. **Decode.** The Android app feeds the stream to a MediaCodec hardware decoder rendering straight to a SurfaceView,
   low-latency mode where the device offers it.
6. **Input.** Touch and pen events go back over a WebSocket on a second port and are injected through uinput devices
   (touchscreen, pen tablet with pressure/tilt/eraser/button, absolute pointer, and a touchpad in touchpad mode). The daemon then
   maps the devices onto the virtual output, per desktop (`input/mapping.rs`), so coordinates land on the right screen.
7. **Latency loop.** Every frame carries a sequence number; the app echoes it when the frame reaches the screen, and the daemon
   logs p50/p95 end-to-end.

## Protocol

**Video (TCP, loopback, port 8890 + 2·slot).** The client first sends the 64-character hex session token. Then the server sends
packets of `u32 length (big-endian)`, `u8 type`, payload: type 0 is codec configuration (SPS/PPS, plus VPS for HEVC), type 1 is
a frame with a `u32` sequence number before the Annex B data.

**Input (WebSocket, loopback, port 8891 + 2·slot).** JSON messages. The first must be `{"type":"auth","token":"…"}`; the server
then replies with a greeting
`{"status":"connected","width":…,"height":…,"codec":"h264"|"hevc","pen_only":bool}` and repeats it whenever the mode changes.
Client messages:

```json
{"type":"touch","x":0.5,"y":0.3,"pressure":1.0,"action":0,"slot":0}
{"type":"pen","x":0.5,"y":0.3,"pressure":0.8,"tilt_x":12.0,"tilt_y":-3.0,"eraser":false,"action":2}
{"type":"resolution","width":2960,"height":1848,"width_mm":314,"height_mm":195}
{"type":"config","bitrate":20000,"fps":60}
{"type":"mode","pen_only":true}
{"type":"rendered","seq":1234,"decode_us":14200}
```

Coordinates are normalised 0–1; tilt is in degrees; touch actions are 0 down, 1 up, 2 move; pen actions add 3 hover, 4 hover exit,
5/6 stylus button down/up. The full set of messages (touchpad frames, gestures, settings, notices) is in `input/protocol/` on the
host and `input/TouchCapture.kt` on the tablet.

## Security model

Loopback-only ports, a per-run random token required before any data flows, private runtime directory for the FIFO and token, no
network traffic except an optional update check. Details and threat model in [SECURITY.md](../../SECURITY.md).
