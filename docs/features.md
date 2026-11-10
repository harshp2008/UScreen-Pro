<!--
  docs/features.md: every feature of UScreen Pro, grouped by what the user does, with where it lives and how far it has been verified.
  Owns the feature list that README.md and the site's landing page summarise; configuration keys are in the site's Configuration page,
  the history of each feature in experiments_and_findings/.
-->

# Features

Everything UScreen Pro does, in one place. The columns say where to find it and **what has been checked**:

- **run** means it was run on the real setup: Ubuntu 24.04, GNOME 46 on Wayland, a Samsung Galaxy Tab S9 FE (SM-X616B), usually over USB.
- **inherited** means it comes from [UScreen by majmichu1](https://github.com/majmichu1/UScreen) and has not been re-run here.
- **built** means it was built and its logic tested, but its visible effect was not watched on the real thing.
- **code only** means the code exists and has unit tests or was built, but it has not been run on the real thing.
- **experimental** means it works in the cases tried and is labelled as such in the app.

Nothing is claimed for another desktop, distribution or tablet than the one above.

## 1. Three modes

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Extended Display** | A virtual monitor (EVDI) is created, captured, hardware-encoded and streamed to the tablet; touch and pen come back | `host/src/capture/`, `host/evdi/` | run (inherited core, re-tuned) |
| **Touchpad** | The tablet is a laptop touchpad: move, tap to click, tap-and-drag, two-finger scroll, pinch, three- and four-finger swipes, palm rejection. Nothing is streamed | `TouchpadEngine.kt`, `host/src/input/uinput/pad.rs` | run |
| **Stylus** | A pen tablet for the screen you are already looking at: pressure, tilt, eraser, side buttons, no display latency. Nothing is streamed | `host/src/input/ghost/` | run |
| **Mode per device** | Each tablet has its own mode; the global setting is the default (`input_mode`) | `config/modes.rs` | run |

## 2. The pen

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Mouse style** | The pen is a pointer that clicks and drag-selects; two side buttons can be right click, middle click or nothing | `ghost/pointer.rs` | run |
| **Graphics tablet style** | The pen is a pressure + tilt + eraser tablet device (`UScreen Pen`) that Krita, GIMP and Blender see as a tablet | `ghost/route.rs` | run (libinput level; Krita not installed) |
| **Dynamic style** | One pen, both behaviours: a tablet in drawing apps, a mouse everywhere else, chosen per app from the focused window | `ghost/auto.rs`, GNOME extension | run, **experimental, GNOME only** |
| **Route hotkey / CLI** | `Ctrl+Alt+P` or `uscreen toggle-tablet` flips the focused app between tablet and mouse | extension `penRouting.js` | run (CLI); hotkey needs the extension |
| **Ghost tracking** | A visible cursor always follows the pen, because tablet-class events hide the pointer on Wayland | `ghost/mod.rs` | run |
| **Hover park filter** | A pen resting on the glass stops taking the cursor from the mouse | `input/stillness.rs` | run (injected rest only) |
| **Click slop** | A small zone around the first touch swallows wobble, so a tap on selected text stays a tap | `ghost/auto.rs` | run |
| **Click hold time** | A tap holds the button for a real click's duration (`pen_click_hold_ms`) so apps that ignore instant clicks accept it | `ghost/` | run |
| **Pointer snapping** | The pointer is snapped to whole physical pixels on GNOME; this, found by replaying a recorded pen stream, is what removed the ghost cursor | experiment 103 | run |
| **Pen menu (cut, copy, paste, select all, undo)** | Hold the pen still: a small menu opens at the pen, only where it makes sense (a selection or clipboard text; not in a title bar or on the desktop). Text or icon buttons, adjustable hold time | `input/menu/`, `penMenu.js`, `menu/` | run |
| **Menu drawn inside the Shell** | On GNOME it is drawn by the extension, so it is not a window and does not change focus; a GTK 4 window is the fallback | `penMenu.js`, `menu/` | run |
| **Pen never reports zero pressure** | A touching pen always sends at least 1, so light taps are not dropped by libinput | `input/events/pen.rs` | inherited |
| **Lift sends the last position** | The pen's last sample is delivered before it leaves proximity | `input/uinput/` | inherited |
| **Physical drawing tablets** | A plugged-in XP-Pen, Wacom or Huion is read through evdev and driven through the same pen path; optional exclusive grab | `input/physical.rs` | run (one XP-Pen) |
| **Palm rejection** | Fingers are ignored while the pen is in play, and anything a finger left pressed is lifted | `PalmFilter.kt` | inherited, run |
| **Touch input switch** | Turn fingers off and keep the pen, for drawing with a resting hand | settings | inherited |
| **S Pen button** | Read from every pen event, including while hovering | `TouchCapture.kt` | inherited |
| **Flight recorder** | `uscreen debug-dump` writes the last few thousand pen samples, for bugs that cannot be reproduced on demand | `input/recorder.rs` | run |

## 3. The touchpad

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Native gestures** | The tablet's raw fingers are replayed on a virtual multi-touch touchpad (`UScreen Touchpad`), so GNOME and KDE handle swipes and pinches themselves | `uinput/pad.rs`, `TouchCapture.kt` | run (single finger from the tablet; multi-finger by scripted replay) |
| **Fallback gestures** | On X11 and desktops without libinput gestures the tablet recognises gestures and the host sends keys | `TouchpadEngine.kt` | code only |
| **Sensitivity** | 50 to 200 percent | settings | run |
| **Right click method** | Bottom-right zone (like a clickpad) or two-finger tap | `TapToClick.kt` | run |
| **Touchpad shape** | Full screen, standard rectangle or wide rectangle | `TouchpadShape.kt` | run |
| **Haptics** | Light vibration on clicks | `Haptics.kt` | inherited |
| **Wake nudge** | A finger touching down nudges the pointer by a pixel so a hidden cursor reappears | `input/events/touchpad.rs` | built; the cursor was not watched on screen |

## 4. Display and performance

| Feature | What it does | Where | Status |
|---|---|---|---|
| **EVDI virtual display** | The tablet is a real monitor in the display settings; its EDID carries the tablet's real size and resolution | `display/`, `host/evdi/` | run |
| **Hardware encoding** | NVENC, VAAPI (including `hevc_vaapi` for Intel and AMD) and `libx264`; the daemon tests that an encoder can really encode a frame and falls back by itself | `capture/encoders.rs` | run (VAAPI/HEVC); NVENC inherited |
| **HEVC and 10-bit** | H.264 or HEVC, optional Main10 for smoother gradients | encoder settings | run (HEVC VAAPI) |
| **Low-latency tuning** | Tuned HEVC reached a 24.5 ms median encode-to-display time at 60 fps and 2560x1600, level with H.264, after the Android decoder was given its low-latency hints | experiment 18 | run (one tablet) |
| **Frame pacing** | High-resolution timers lock the stream to its frame rate; the old bursts to 78 fps and drops to 20 are gone | `host/evdi/`, experiment 16 | run |
| **Idle floor and Max performance mode** | An idle screen still sends a steady trickle; in the `performance` power profile it keeps a continuous 60 fps | `capture/`, experiment 14 | run |
| **Ghost cursor and boundary lag fixed** | A boundary-sync engine removed the cursor burned into the picture and the lag moving the mouse between screens | `host/evdi/`, experiment 5 | run |
| **Stream scale** | Send a half or quarter of the pixels for less decode time | `stream_scale` | inherited |
| **Per-device encoder overrides** | Each tablet can use its own encoder, codec, frame rate, bitrate and position | `[tablets.<serial>]` | run |
| **Latency measurement** | `uscreen doctor --latency` reports stage-by-stage timings and percentiles | `capture/latency.rs`, `cli/doctor/latency.rs` | run |
| **Software cursor option** | A compatibility mode when the fast cursor path misbehaves | `software_cursor` | run |
| **Keep-alive display** | Keep the virtual display after the tablet is unplugged | `keep_alive` | run |
| **Screen placement** | Right, left, above or below your real screens | `position` | run (GNOME) |
| **Several tablets** | Up to four, each its own screen, isolated from each other (updating one does not disturb another) | `daemon/session/` | run (two devices) |
| **Remembered panels** | The panel each tablet reported is stored under its serial, so the right display is built on plug-in | `config/profile.rs` | inherited |

## 5. Connection: USB, Wi-Fi, Tailscale

| Feature | What it does | Where | Status |
|---|---|---|---|
| **USB through adb** | Plug in; the daemon forwards the ports, launches the app and builds the display | `net/adb/` | run |
| **Wired or wireless ADB setup** | The wizard sets up either path: USB first, then wireless from the cable in one click | wizard, `net/adb/wifi.rs` | run |
| **Wireless pairing** | Android 11+ pairing by QR code or by pairing code, using mDNS discovery of the TLS endpoint | `net/pairing.rs`, wizard | code only for the QR path on a clean network |
| **Legacy Wi-Fi fallback** | Android 10 and older, behind an explicit switch and a unique port per device | `allow_legacy_fallback` | code only |
| **Automatic Wi-Fi provisioning** | Plug in once; Wi-Fi is ready for when the cable comes out | `auto_enable_wifi_adb` | run |
| **Transport preference** | USB / Auto / Wi-Fi per device; the cable wins when both are there | device settings | run |
| **Tailscale** | Connect over a Tailscale address when the LAN will not do | wizard | **experimental** |
| **Router-isolation diagnosis** | When the tablet cannot reach the computer the wizard says why (MAC randomization, AP isolation) and how to fix it, instead of spinning | `ROUTER_BLOCKED`, wizard | run |
| **Reconnect that does not give up** | Sockets retry every 2 s; the app can tell "host offline" from "waiting" | `HostLink.kt` | run |
| **Session token** | Both ports are loopback-only and need a per-run token | `require_token` | inherited |

## 6. Settings that follow you

| Feature | What it does | Where | Status |
|---|---|---|---|
| **One file, two editors** | The desktop app and the tablet app edit the same `config.toml`; keys you add by hand are kept | `config/` | run |
| **Global default + per-device override** | Click a grey value on a device page to override it for that device only | `[tablets.<serial>]` | run |
| **Last change wins** | A global change clears the per-device override; a device change applies to that device. The file is watched, so a hand edit reaches the tablet in about a second | `config/options/`, `App.tsx` | run |
| **Parity** | Every tablet setting is also a row in the desktop app, and the other way round | `PowerSettings.tsx`, `SettingsSheet.kt` | run |
| **Settings apply by restarting only what changed** | Saved settings restart the affected pipeline of that device, not the daemon | `daemon/session/` | run |

## 7. Battery and power

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Idle dimming** | The tablet dims while the pointer is away from it and wakes on any touch or pen input | `config/options/idle.rs`, `ScreenDimmer` | run |
| **Fade curves** | Choose how the dim and undim fade: presets, or a custom Bezier curve with a live preview, saved as your own presets | `CurveEditor.tsx`, `FadeCurve` | run |
| **Battery saver on disconnect** | After two minutes without the computer the tablet stops retrying and dims to 10% | `PowerSupport.kt` | run |
| **Optional lock** | Battery saver can also lock the tablet (asks for Device Admin only if you turn it on) | `battery_saver_lock` | run (lock accepted; screen stayed on, timeout "never") |
| **Black screen in Stylus mode** | The explanation shows for a few seconds, then the screen goes black so an OLED is not left lit | `blank_pen_only` | inherited |
| **Wake on touch** | Touching the dimmed tablet wakes it | `ConnectionScreen.kt` | run |
| **Lock-screen aware** | Optionally keeps the app over the lock screen and forwards touches while locked | settings | built |
| **Keep connected in background** | Optional | settings | code only |

## 8. The tablet app

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Redesigned app** | Material 3 theme matching the desktop app, light, dark or system | `ui/Theme.kt` | run |
| **Settings in four cards** | Display and input, Connection, Power and battery, Advanced | `ui/SettingsSheet.kt` | run |
| **Immersive mode** | Hide the settings handle and draw under the cutout | settings | run |
| **Safe area** | Non-streaming UI respects the display cutout | `ui/ConnectionScreen.kt` | run (on a phone) |
| **Orientation** | Automatic (reads the tilt sensor itself), camera up or camera down | `OrientationPolicy.kt` | inherited |
| **Stats overlay** | fps and bandwidth, shown in Extended Display mode only | `ConnectionScreen.kt` | run |
| **Decoder recovery** | A stalled or refusing decoder steps down a ladder instead of freezing | `VideoReceiver.kt` | inherited |
| **Pairing guide** | A screen that walks through Wireless debugging and opens the right Android settings | `ui/PairingGuideScreen.kt` | run |
| **Clear errors on the tablet** | "Host Offline" with Retry, "Debugging Disabled" with a button to Developer Options, "Connection Paused by Host", an update screen while the app is replaced | `ui/Onboarding.kt`, `HostOverlays.kt` | run |
| **Host shutdown notice** | When the daemon stops it tells the tablet, so the tablet says so at once | `net/stream.rs` | run |
| **Uninstall and Identify** | Remove the app from the tablet; post a heads-up notification on the tablet ("This is your target device") to tell which physical device a row is | `IdentifyReceiver.kt` | run (Identify); Uninstall built |

## 9. The desktop app

| Feature | What it does | Where | Status |
|---|---|---|---|
| **New app** | Tauri 2 + React, Material 3 look, frameless window, light and dark themes | `uscreen-pro-gui/` | run |
| **Device wizard** | Five steps: Prepare Device, Connect USB, Install, Wireless, Finish, with pictures of the USB-debugging steps | `AddDeviceWizard.tsx` | run |
| **Device pages** | Identity and nickname, transport, display and video, input mode, power, app locking, with an override per row | `views/DeviceSettings.tsx` | run |
| **Global settings** | The defaults every device inherits | `views/GlobalSettings.tsx` | run |
| **System health** | Daemon state, control socket, build (with a *dirty* flag), device tools, Restart ADB, Force Reinstall APK | `views/Health.tsx` | run |
| **Start, stop, pause** | A real Pause for one device; stopping the daemon keeps the window open | `daemon.rs` | run |
| **Forget a device** | Remove a device and its profile | `forget.rs` | run |
| **Start with desktop** | Toggle the systemd user unit | settings | built |
| **Dev preview** | `npm run dev` runs the whole UI against mock data, so the screens can be developed without a tablet | `lib/devMock.ts` | run |

## 10. Updates

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Automatic app update** | When a paired tablet connects, the daemon compares the installed app's version with its own and installs the matching APK itself, through a queue (two at a time so several tablets do not saturate USB) | `net/installer.rs` | run |
| **Build stamping** | Every build carries its git commit and a *dirty* flag, so a half-edited build is never mistaken for a release | `build.rs`, Gradle | run |
| **Per-device updater** | Updating one tablet leaves the others streaming; the tablet shows an "updating" screen | `net/notices.rs` | run |
| **Version mismatch banner** | A host/app mismatch is named on the tablet | `ConnectionScreen.kt` | run |
| **New release notice** | Once a day (switchable) the tray and `uscreen doctor` say a newer release exists; nothing installs itself | `daemon/update.rs` | inherited |

## 11. Reliability and error handling

| Feature | What it does | Where | Status |
|---|---|---|---|
| **Compositor safety guard** | The virtual monitor is not created until the compositor is up and idle; hotplugs are rate-limited; a stalled compositor is detected; a boot-loop guard stops the display from starting after an unclean end. `uscreen reset-guard` clears it | `daemon/guard/` | run |
| **Freeze fixes on Intel** | A DRM race between EVDI and i915 that froze the desktop on plug-in was traced and prevented (module load order, udev rule) | experiments 2 and 56 | run |
| **`uscreen doctor`** | One command checks the machine, the tablet, the display and the settings, and gives a hint for each failure | `cli/doctor/` | inherited, extended |
| **`uscreen check-encoder`** | Tells whether an encoder can really encode a frame | `capture/encoders.rs` | run |
| **Atomic config writes** | A crash mid-write cannot leave a truncated file | `config/` | inherited |
| **One daemon** | PID and socket handling stop two daemons fighting for the capture device | `config/handler.rs` | run |
| **Tray icon** | State line, a Graphics tablet toggle, Show Window, Stop, and an update notice; the window survives "Stop Daemon" | `daemon/tray.rs` | run |

## 12. Desktop integration

| Feature | What it does | Where | Status |
|---|---|---|---|
| **GNOME Shell extension** | Four jobs inside the Shell: app lock, pen routing, a context probe for the menu, and the pen menu | `extensions/uscreen-app-lock/` | run (GNOME 46) |
| **App locking** | Keep chosen apps on the tablet's monitor | `appLock.js`, Advanced page | run |
| **Desktop backends** | One directory per desktop (GNOME, X11, KDE, Hyprland, COSMIC, generic); a feature that needs a capability does nothing where it is missing | `host/src/desktop/` | GNOME run; others code only |
| **GNOME Shell version table** | One row per Shell version, with a `tested`, `declared` or `unloadable` status and a checker | `compat/versions.js` | run |
| **Input mapping** | Pen and touch are mapped to the tablet's monitor on GNOME (Mutter), X11, KDE, Hyprland and Cinnamon | `host/src/input/mapping.rs` | GNOME run |

## 13. Install and project

| Feature | What it does | Where | Status |
|---|---|---|---|
| **A tested installer** | `./scripts/install.sh` with a plan mode and a dry run; one file per distribution family (Debian, Fedora, Arch, openSUSE, unknown); it reports whether a log out or reboot is needed | `scripts/` | 60 tests, also in containers of four distributions locally |
| **Release packages** | `.deb`, `.rpm`, PKGBUILD and tarball recipes | `packaging/` | code only |
| **Add support with one file** | A distribution is one file, a desktop one directory, a GNOME version one table row | `docs/adding-support.md` | run |
| **Documentation site** | Starlight site with the configuration reference, CLI reference, rustdoc and 62 converted experiment records, built and link-checked in CI | `website/` | run |
| **Every function documented** | Doc comment on every function, header on every file, with a checker for the style | `docs/comment-style.md` | run |
| **Experiments published** | 62 records of what was tried and found, with redaction and no invented facts | `experiments_and_findings/` | run |
| **CI** | Build, installer and docs workflows with caching and path filters | `.github/workflows/` | run |

## What UScreen Pro did not change

The EVDI capture design, the first Android app, the adb transport and the session-token security come from [UScreen](https://github.com/majmichu1/UScreen). Items marked *inherited* above are theirs.
