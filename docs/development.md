<!--
  docs/development.md: building from source, running and testing, the project layout, the command line, encoder tuning and releasing.
  Owns the developer workflow; installing as a user is in docs/installation.md.
-->
# Development

## Building from source

Dependencies: Rust (stable), gcc, `pkg-config`, `libdrm` headers, `ffmpeg`,
`adb`, and the evdi kernel module. See [installation.md](installation.md) for
the per-distribution package names.

```bash
make build            # EVDI helper (C) + Rust daemon + GUI
make install          # copies to ~/.local/bin, installs the systemd user unit
make setup-system     # modprobe.d / modules-load.d / udev rule (sudo)
```

The Android app:

```bash
cd android
./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Or open `android/` in Android Studio.

## Running and testing

```bash
RUST_LOG=uscreen=debug uscreen start      # in a terminal, with the tablet attached
cargo test --release --manifest-path host/Cargo.toml
cargo clippy --release --manifest-path host/Cargo.toml
cd android && ./gradlew lintDebug
```

`scripts/fake-tablet.py` pretends to be a tablet on the loopback ports
(authenticates, reports a resolution, acks frames). With
`USCREEN_FAKE_TABLET=fake1,fake2` and `max_tablets = 2` it exercises a
second pipeline without a second device.

## Project layout

```
host/                   Rust daemon (binary `uscreen`)
  src/main.rs             entry point: parses the CLI and runs the chosen command
  src/cli/                command line: `mod.rs` (clap), `commands.rs` (stop/status/...),
                          `doctor/` (`uscreen doctor`, split into system/tablet/display/settings/latency)
  src/config/             config.toml: `mod.rs` (FileConfig), `sanitize.rs` (clamps and migrates a loaded config),
                          `profile.rs` (per-tablet profile), `modes.rs`, `limits.rs`, `curves.rs`, `clean.rs`;
                          `handler.rs` (daemon control socket, PID file); `options/` (live per-tablet options
                          and the idle-dim loop)
  src/daemon/             the running daemon: `mod.rs` (startup/shutdown), `session/` (tablet sessions and
                          the adb monitor), `guard/` (compositor-freeze safety), `runtime.rs`, `tray.rs`,
                          `update.rs`
  src/capture/            EVDI helper + encoders: frames in, H.264/HEVC packets out (`packetizer.rs`,
                          `encoders.rs`, `latency.rs`, `timing.rs`)
  src/desktop/            one directory per desktop environment (see below), the shared `DesktopBackend`
                          trait in `mod.rs`, and the dispatchers `display.rs` (virtual output) and `scale.rs`
  src/display/            what is not desktop specific: the generated EDID (`edid.rs`) and EVDI card
                          discovery in sysfs (`vdisplay.rs`)
  src/input/              input path: `server.rs` (WebSocket), `protocol/` (wire types, sequence filter),
                          `events/` (one handler per message kind), `menu/` (the long-press pen menu),
                          `ghost/` (pen routing: mouse vs tablet, auto style, click slop), `uinput/` (virtual
                          devices), `stillness.rs` (resting-pen filter), `recorder.rs` (pen flight recorder),
                          `physical.rs` (real drawing tablets), `mapping.rs` (binding devices to outputs)
  src/net/                `adb/` (device discovery, `adb reverse` tunnels, wireless adb, the tablet app),
                          `pairing.rs`, `installer.rs` (APK install queue), `stream.rs` (video server),
                          `notices.rs`
  evdi/                   C helper: `evdi_helper.c` (device discovery, `main`) includes `helper_*.h` (state,
                          signals, X11 boundary sync, timing ring, NV12 conversion, writer, EVDI events)
menu/                   `uscreen-menu`: the pen's long-press menu window (GTK 4), launched by the daemon
uscreen-pro-gui/        Tauri + React desktop app (settings, pairing, status)
  src/                    `App.tsx` (state and routing), `views/`, `lib/` (types, options, `configMerge.ts`,
                          `scroll.ts`), `components/` (`shell/`, `wizard/`, `power/`, `ui/`)
  src-tauri/src/          `adb/` (devices, install, intents), `config.rs`, `daemon.rs`, `forget.rs`, `pairing.rs`
android/                Kotlin/Compose app: `input/` (touch, pen and touchpad), `video/` (MediaCodec decoder),
                          `ui/` (screens); entry points and Android-version helpers in the root package
extensions/             GNOME Shell extension `uscreen-app-lock`: `extension.js` (entry), `lib/` (app lock,
                          pen routing, context probe, pen menu), `compat/` (per-Shell-version code)
packaging/              deb control/postinst, rpm spec, PKGBUILD, udev/modprobe files
scripts/                `install.sh` and `lib/` (`platform.sh`, `evdi.sh`, `tui.sh`, `distros/<family>.sh`),
                          release build, test tools (fake tablet, tablet_inject.py), `tests/` (installer tests)
experiments_and_findings/  numbered write-ups of what was measured and decided
docs/                   this documentation and the existing GitHub Pages site
```

Where to change what: a new **distribution** is one file in `scripts/lib/distros/`, a new **desktop** is one
directory in `host/src/desktop/`, a new **GNOME Shell version** is a row in
`extensions/uscreen-app-lock/compat/versions.js`. See [adding-support.md](adding-support.md).

## Command line

```
uscreen [OPTIONS] [COMMAND]

COMMANDS
  start           start the daemon
  stop            stop the daemon
  status          show daemon status
  list-displays   list EVDI displays
  wifi            put the tablet on the network (--off to undo)
  doctor          diagnose the setup and print fixes

OPTIONS (override ~/.config/uscreen/config.toml for this run only)
  --encoder <NAME>      h264_nvenc, hevc_nvenc, h264_vaapi, libx264
  --fps <N>             frame rate (30–90)
  --bitrate <KBPS>      bitrate ceiling
  --width/--height <N>  capture size (auto_resolution off)
  --quality <Q>         constant-quality target, 12–32, lower is sharper
  --stream-scale <N>    downscale the stream only, 1 = native, 2 = half
  --pen-only            graphics-tablet mode for this run
  --video-port/--input-port <PORT>
  --helper <PATH>, --edid <PATH>
```

## Encoder tuning

- NVIDIA: `h264_nvenc` (default) or `hevc_nvenc` — see the codec section of
  the README for when HEVC and `ten_bit` are worth it.
- AMD/Intel: `h264_vaapi`, constant-quality via `quality`.
- CPU: `libx264`, `ultrafast`/`zerolatency`; expect 30 fps at most on a laptop.

`quality` is what governs picture quality; `bitrate` is only a ceiling for
bursts. On a static desktop the stream sits far below it.

## Optional in-process encoder

```bash
cargo build --release --manifest-path host/Cargo.toml --features inproc-encoder
```

Encodes through libavcodec in-process instead of an `ffmpeg` child: same
latency, about one CPU core less, and keyframes on demand. Needs the ffmpeg
development headers (`ffmpeg-devel` from RPM Fusion on Fedora,
`libavcodec-dev libavformat-dev libavutil-dev libswscale-dev` on Debian). On
atomic distributions build inside a container (`distrobox`); the binary links
against the host's ffmpeg at runtime. `ten_bit` is not available on this path.

## Release APK

```bash
cd android
keytool -genkeypair -keystore uscreen-release.keystore -alias uscreen \
        -keyalg RSA -keysize 2048 -validity 10000
# keystore.properties: storeFile / storePassword / keyAlias / keyPassword
./gradlew assembleRelease     # app/build/outputs/apk/release/app-release.apk
```

The keystore and `keystore.properties` are gitignored. The same key must sign
every future release or users cannot update in place.

## Releasing (maintainers)

Binaries are built in a Debian 12 container so they run on any current glibc:

```bash
distrobox create --image debian:12 --name uscreen-build
# inside: build-essential pkg-config libdrm-dev git dpkg-dev fakeroot rpm curl,
#         the X11/Wayland dev packages for the GUI, and rustup

GH_TOKEN=... make publish NOTES=release-notes.md
```

`make publish` runs `scripts/build-release.sh` and `packaging/build-packages.sh`,
refuses to continue unless all five release files exist, creates the GitHub
release for the already-pushed tag (titled *UScreen X.Y.Z — USB second monitor
for Linux with S Pen support*) and uploads everything plus `SHA256SUMS`.

Before that, for a new version:

1. Bump `VERSION` in the Makefile, `version` in both `Cargo.toml` files and
   `versionCode`/`versionName` in `android/app/build.gradle.kts`.
2. Add a `## X.Y.Z — YYYY-MM-DD` entry to `CHANGELOG.md`.
3. `make release-metadata DATE=YYYY-MM-DD` (or
   `scripts/update-release-metadata.sh X.Y.Z YYYY-MM-DD`): rewrites the
   version, dates and package names in `docs/index.html` (including the
   JSON-LD), `docs/llms.txt`, `docs/sitemap.xml` and `CITATION.cff`.
4. Commit, tag `vX.Y.Z`, push both.

`make publish` re-checks all of it (`update-release-metadata.sh --check`,
the Cargo/Gradle versions, the changelog entry) and stops with a message
naming the stale file rather than publishing a page that still says the
previous version. The date defaults to today; `RELEASE_DATE=YYYY-MM-DD` overrides it.
