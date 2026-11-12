<!--
  docs/architecture/platform-modularity-plan.md: the audit of platform-specific code and the plan to put it behind one interface per capability.
  Owns: the capability table, every tagged place, and what each needs to be platform neutral. Does not own the final design
  (docs/architecture/platform-modularity.md, written when the plan is implemented).
-->

# Platform modularity plan

Written in step 4 of the cleanup, before any code moved. Step 5 implemented the part of it that has more than one backend
(the display capability); the rest is a seam with a doc comment. What was actually done is in
[platform-modularity.md](platform-modularity.md).

## How the code is tagged

Every place that depends on a desktop, a compositor, a distribution, an init system, a kernel interface or an Android API carries a
comment of the form `PLATFORM(<name>): <what it does>` (`//` in Rust, Kotlin, TypeScript, JavaScript and C, `#` in shell, Python,
systemd and desktop files, `<!-- -->` in XML). Find all of them with:

```bash
git grep -n "PLATFORM("             # everything
git grep -n "PLATFORM(hyprland)"    # one platform
```

Names in use: `gnome`, `x11`, `wayland`, `kde`, `hyprland`, `cosmic`, `linux`, `systemd`, `dbus`, `evdi`, `uinput`, `android`,
and the distribution families `debian`, `fedora`, `arch`, `suse`. A tag marks the function or file where the dependency lives;
it does not claim the whole file is platform code. 137 tags exist at the time of writing.

## The model to copy

`host/src/desktop/` already has the target shape for two capabilities:

* **Focus, pointer context, pen menu**: the trait `DesktopBackend` (`desktop/mod.rs`) with one implementation per desktop
  (`gnome`, `x11`, `generic`) and `detect()` choosing one. Features read `Capabilities`, never the desktop's name.
* **Interface scale**: `desktop/scale.rs` holds an enum of backends, `backend()` picks one, and `apply(target, percent)` calls the
  one module `desktop/<name>/scale.rs` whose function has the same signature in every desktop.

Everything else that differs by desktop (enabling the virtual output, mapping devices to it, finding its area) was an `if`
ladder in three files. The plan below puts it behind the same pattern.

## Capability by platform

Status: **yes** = implemented and used; **partly** = a piece exists; **no** = nothing, and the feature quietly does less;
**n/a** = the platform has no such concept. "Untested" means the code is written and its command building or parsing is unit
tested, but it has not been run on that desktop by the maintainer.

| Capability | GNOME (Wayland) | X11 | KDE Plasma | Hyprland | COSMIC | Other (Sway, ...) | Windows / macOS |
|---|---|---|---|---|---|---|---|
| Frame capture (EVDI virtual display) | yes | yes | yes | yes, untested | yes, untested | untested | no (EVDI is a Linux kernel module) |
| Enable, disable and place the virtual output | yes (the compositor does it) | yes (xrandr) | yes (kscreen-doctor, inline in `capture/display.rs`) | yes, untested | partly (enable and disable, no placement) | no | no |
| Map touch and pen devices to the output | yes (gsettings; Cinnamon too) | yes (xinput) | yes (KWin D-Bus), untested for current Plasma | yes, untested | no | no | no |
| Pointer area of the output (for ghost tracking) | yes | n/a (one core pointer) | yes (kscreen-doctor) | yes, untested | no | no | no |
| Interface scale of the virtual display | yes (GNOME 46) | n/a | yes, untested | yes, untested | no | no | no |
| Focus tracking (art-app pen route) | yes (Shell extension) | yes (`_NET_ACTIVE_WINDOW`) | no (generic) | no (generic) | no (generic) | file written by a helper, if any | no |
| Pointer context and pen menu in the shell | yes (extension) | no | no | no | no | no | no |
| Window placement at the pointer (menu window) | yes, needs `spawn-at` | no | no | no | no | no | no |
| Pointer tracking for idle dimming | yes (`spawn-at` pointer watcher) and X11 boundary thread in the EVDI helper | yes (helper) | partly (helper only on X11) | no | no | no | no |
| Compositor freeze watchdog | yes | no | no | no | no | no | no |
| Native touchpad gestures (libinput) | yes | no (host-side gestures) | yes | no | no | no | no |
| On-screen keyboard suppression | no | n/a | yes | no | no | no | no |
| Input injection (uinput) | yes | yes | yes | yes | yes | yes | no (Linux only) |
| Physical tablet pass-through (evdev) | yes | yes | yes | yes | yes | yes | no |
| Tray icon (StatusNotifierItem) | needs the AppIndicator extension | yes (with a tray) | yes | with a bar that has a tray | with a tray | with a tray | no |
| Autostart and service control | systemd user unit | systemd | systemd | systemd | systemd | systemd only; no OpenRC or runit unit | no |
| GUI auto-scale suggestion (`display_auto_percent`) | yes (Mutter) | no | no | no | no | no | no |
| Installer | Debian, Fedora, Arch, openSUSE families; unknown families get the files only | same | same | same | same | same | no |

Android side: one app, minimum API level in `android/app/build.gradle.kts`. Version-specific branches (Android 8.1, 11, 13, 14)
are tagged `PLATFORM(android)`; there is no other mobile platform.

## Tagged places, by capability

Each group below lists every tag in it, then answers the five questions for the group: the neutral signature, what a new
platform implements, how the backend is chosen, and what happens when there is none.

### 1. Desktop detection and backend choice

| Where | Platform | What the code does |
|---|---|---|
| `host/src/desktop/mod.rs:130` | linux | chooses the backend from XDG_SESSION_TYPE (X11 first) and XDG_CURRENT_DESKTOP (GNOME); every other desktop gets the generic backend |
| `host/src/desktop/mod.rs:149` | x11 | reads XDG_SESSION_TYPE; X11 has one core pointer, so ghost-pointer tracking is switched off |
| `host/src/desktop/mod.rs:155` | linux | files exchanged with the Shell extension and the CLI live under $HOME/.local/share/uscreen |
| `host/src/desktop/scale.rs:41` | linux | detection order is Hyprland, GNOME, X11, then "kscreen-doctor is on PATH" as the KDE signal; any other desktop gets no scale support |

* **Neutral signature.** `fn detect() -> Box<dyn DesktopBackend>` (exists) and `fn backend() -> Option<Backend>` (exists in
  `scale.rs`). Today there are two detectors with two different orders (`detect()` for focus, `scale::backend()` for the
  rest) and several more `active()` functions, one per display module.
* **A new platform implements.** One `active() -> bool` (cheap, no side effects) in its `desktop/<name>/` directory.
* **Selection.** Runtime, from `XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP` and whether the needed tool is on `PATH`. Step 5
  replaces the `active()` ladders with one `desktop::display::backend()` that returns an enum, so the order is written once.
* **Fallback.** The `generic` backend and no display backend: the daemon runs, streams, and injects input, and logs once that
  the desktop is not known; `uscreen doctor` says which features are missing.

### 2. Virtual display, input mapping and pointer area

| Where | Platform | What the code does |
|---|---|---|
| `host/src/capture/display.rs:10` | evdi | explains a missing evdi kernel module or missing devices by reading /sys/module/evdi and /sys/devices/evdi |
| `host/src/capture/display.rs:71` | linux | if-ladder over desktop backends: Hyprland, COSMIC, GNOME (does it by itself), X11, then inline kscreen-doctor code for KDE |
| `host/src/capture/display.rs:72` | linux | if-ladder over desktop backends: Hyprland, COSMIC, GNOME (does it by itself), X11, then inline kscreen-doctor code for KDE |
| `host/src/capture/display.rs:73` | linux | if-ladder over desktop backends: Hyprland, COSMIC, GNOME (does it by itself), X11, then inline kscreen-doctor code for KDE |
| `host/src/capture/display.rs:283` | linux | same ladder as enable_evdi_display; the KDE branch is inline kscreen-doctor code |
| `host/src/display/vdisplay.rs:3` | evdi | EVDI virtual cards are found through /sys/class/drm and /sys/devices/evdi (Linux sysfs) |
| `host/src/display/vdisplay.rs:51` | evdi | connector names of EVDI cards come from /sys/class/drm; a real DVI monitor has the same name pattern |
| `host/src/desktop/cosmic/display.rs:12` | cosmic | virtual display on COSMIC through the cosmic-randr command |
| `host/src/desktop/cosmic/mod.rs:2` | cosmic | COSMIC has output enable/disable only; there is no input mapping or scale backend |
| `host/src/desktop/gnome/display.rs:17` | gnome | virtual display on GNOME: Mutter DisplayConfig over busctl, and device-to-monitor mapping through gsettings (also used for Cinnamon) |
| `host/src/desktop/gnome/display.rs:275` | gnome | Cinnamon keeps the same per-device schema under org.cinnamon.desktop.peripherals |
| `host/src/desktop/hyprland/display.rs:11` | hyprland | virtual display on Hyprland: monitors, placement and input mapping through hyprctl |
| `host/src/desktop/hyprland/mod.rs:2` | hyprland | Hyprland has display handling only; focus and the pen menu use the generic backend |
| `host/src/desktop/kde/display.rs:14` | kde | virtual display on KDE Plasma: kscreen-doctor for outputs, KWin D-Bus (busctl) for input device properties |
| `host/src/desktop/kde/mod.rs:4` | kde | KDE Plasma has display handling and the keyboard policy only; focus and the pen menu use the generic backend |
| `host/src/desktop/x11/display.rs:15` | x11 | virtual display on X11: RandR through the xrandr command, input mapping through xinput |
| `host/src/input/mapping.rs:12` | kde | finds the laptop panel with kscreen-doctor |
| `host/src/input/mapping.rs:52` | kde | reads a property of a KWin input device over D-Bus |
| `host/src/input/mapping.rs:63` | linux | if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case) |
| `host/src/input/mapping.rs:64` | linux | if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case) |
| `host/src/input/mapping.rs:65` | linux | if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case) |
| `host/src/input/mapping.rs:66` | linux | if-ladder over desktop backends: Hyprland, GNOME, X11, then KDE/KWin as the fallback (COSMIC has no case) |
| `host/src/input/mapping.rs:288` | linux | area lookup differs per desktop: Hyprland, GNOME, X11 (none) and kscreen-doctor for the rest |
| `host/src/input/mapping.rs:338` | kde | output rectangle from kscreen-doctor JSON |
| `host/src/input/mapping.rs:382` | kde | runs kscreen-doctor -j and parses its JSON |

* **Neutral signature** (one module per desktop, same names; step 5 puts the dispatch in `desktop/display.rs`):

  ```text
  pub fn active() -> bool
  pub async fn enable_output(names: &[String], position: Position)
  pub async fn disable_output(names: &[String])
  pub async fn map_devices(device_names: &[String], output: &str) -> bool      // or the richer form used today
  pub async fn output_area(output: &str) -> Option<(f64, f64, f64, f64)>       // fractions of the whole desktop
  ```
* **A new platform implements.** The five functions above in `desktop/<name>/display.rs`, using whatever the compositor offers
  (a D-Bus interface, a command-line tool, a protocol). `names` are the EVDI connector names from
  `display::vdisplay::evdi_connectors`, never a guess from the name pattern.
* **Selection.** Runtime. The order today is Hyprland, COSMIC, GNOME, X11, then KDE as the fall-through in three files
  (`capture/display.rs`, `input/mapping.rs`, `cli/doctor/display.rs`). KDE has no module of its own for enabling and mapping:
  its code is inline in the first two. Step 5 moves it into `desktop/kde/display.rs` so KDE is a peer.
* **Fallback.** No backend: the output is left as the compositor made it, the devices are not mapped (touch and pen then
  address the whole desktop) and the daemon logs a warning that says so. Today a missing KDE tool produces that warning; a
  desktop with no case at all (Sway, Windows, macOS) falls into the KDE path and fails the same way.

### 3. Interface scale

| Where | Platform | What the code does |
|---|---|---|
| `host/src/desktop/gnome/scale.rs:11` | gnome | interface scale through Mutter ApplyMonitorsConfig (busctl), as a temporary layout |
| `host/src/desktop/hyprland/scale.rs:2` | hyprland | scale through a hyprctl "monitor" keyword rule |
| `host/src/desktop/kde/scale.rs:2` | kde | scale through kscreen-doctor output.NAME.scale.S |
| `uscreen-pro-gui/src-tauri/src/display.rs:32` | gnome | asks Mutter (busctl DisplayConfig.GetCurrentState) for the preferred scale; on other desktops the call fails and there is no answer |

* **Neutral signature.** `pub async fn apply(connectors: &[String], percent: u32) -> bool` (exists; GNOME takes the slot instead
  of the connectors because it plans a whole layout). `0` means automatic; the call is cheap and harmless when nothing changes.
* **A new platform implements.** `desktop/<name>/scale.rs::apply`, plus one line in `scale::Backend`, `backend()` and `apply()`.
* **Selection.** Runtime, `scale::backend()`. It duplicates the detection of group 2 and is folded into it in step 5.
* **Fallback.** `supported()` is false, the setting does nothing and the settings page says the desktop cannot do it.

### 4. Focus, pointer context, pen menu, shell extension

| Where | Platform | What the code does |
|---|---|---|
| `extensions/uscreen-app-lock/compat/index.js:6` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/compat/monitors.js:7` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/compat/versions.js:11` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/extension.js:17` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/lib/appLock.js:6` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/lib/contextProbe.js:5` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/lib/dataFiles.js:5` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/lib/penMenu.js:5` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `extensions/uscreen-app-lock/lib/penRouting.js:5` | gnome | runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json |
| `host/src/desktop/generic/mod.rs:4` | linux | fallback for every desktop without a backend: focus only from a file a helper script may write |
| `host/src/desktop/gnome/mod.rs:5` | gnome | focus, pointer context and the pen menu come from the uscreen-app-lock Shell extension through files in ~/.local/share/uscreen; GNOME Shell on Wayland only |
| `host/src/desktop/gnome/pointer.rs:8` | gnome | pointer position and monitor layout come from the spawn-at tool (a GNOME Shell extension) |
| `host/src/desktop/gnome/spawn_at.rs:4` | gnome | window focus and placement at the pointer through the spawn-at tool |
| `host/src/desktop/x11/mod.rs:3` | x11 | focus from the _NET_ACTIVE_WINDOW root property (x11rb) |
| `menu/src/main.rs:14` | linux | GTK 4 window; the daemon starts it and reads its answer from a file |

* **Neutral signature.** `trait DesktopBackend` (exists): `poll_focus`, `context_at_pointer`, `focused_window`, `focus_window`,
  `show_menu`, `wait_menu_choice`, `spawn_at_pointer`, plus `Capabilities`.
* **A new platform implements.** `desktop/<name>/mod.rs` with `impl DesktopBackend`; only the methods whose capability flag it
  sets. For a Wayland compositor without a window-list protocol this means a small extension or plugin on the compositor side,
  as GNOME has, because a Wayland client cannot ask for the focused window.
* **Selection.** Runtime, `desktop::detect()`.
* **Fallback.** `generic` returns `Capabilities::default()`: focus comes only from a file, the pen uses the route chosen in
  settings, and there is no menu. This is already how KDE, Hyprland and COSMIC behave.

### 5. Input injection and physical tablets

| Where | Platform | What the code does |
|---|---|---|
| `host/src/cli/doctor/latency.rs:72` | uinput | opens /dev/uinput directly to measure injection latency |
| `host/src/config/options/device.rs:203` | wayland | reads XDG_SESSION_TYPE and XDG_CURRENT_DESKTOP to decide whether the tablet may send raw touchpad frames |
| `host/src/config/options/device.rs:229` | wayland | libinput gestures reach apps only on a Wayland session of GNOME or KDE; X11 and other desktops get host-side gestures |
| `host/src/input/physical.rs:260` | linux | reads a physical tablet through evdev (/dev/input/event*), which needs access granted by the udev rule |
| `host/src/input/physical.rs:386` | linux | udev rules text for 60-uscreen-tablet-access.rules; the rule file name must sort before 73-seat-late.rules |
| `host/src/input/uinput/consts.rs:6` | linux | Linux input-event-codes.h values (EV_*, BTN_*, ABS_*) |
| `host/src/input/uinput/device.rs:5` | uinput | Linux uinput ABI: struct input_event and the ioctl numbers used to create virtual devices |
| `host/src/input/uinput/mod.rs:10` | uinput | virtual pen, touch and pointer devices created through /dev/uinput (needs the uinput module and permission) |

* **Neutral signature.** Today `InjectDevices` (`input/uinput/`) is the one injector, a Linux type. The seam is a trait with
  `move_pointer`, `button`, `pen`, `touch`, `key` operations that `InjectDevices` would implement; it is **not** introduced yet,
  because there is exactly one implementation and the methods are shaped by uinput's event model.
* **A new platform implements.** An injector for its OS (SendInput on Windows, CGEvent on macOS) and a way to create a
  virtual pen and touch device, which those systems do not offer to user programs without a driver.
* **Selection.** Compile time (`cfg(target_os)`) once a second injector exists; today none is needed.
* **Fallback.** No injector: the daemon refuses to start the input server and says "/dev/uinput is not available" (the doctor
  checks it). Video still works.

### 6. Capture helper (EVDI) and process control

| Where | Platform | What the code does |
|---|---|---|
| `host/evdi/evdi_helper.c:33` | evdi | finds the EVDI card by scanning /sys/devices/platform/evdi.*/drm (Linux sysfs) |
| `host/evdi/helper_boundary.h:8` | x11 | pointer-crossing detection uses an X11 connection (XRandR); the connection is NULL on a Wayland-only session |
| `host/evdi/helper_events.h:8` | evdi | callbacks and event loop of libevdi (mode change, update ready, DPMS, cursor) |
| `host/evdi/helper_signals.h:8` | linux | POSIX signal handling |
| `host/evdi/helper_state.h:8` | evdi | holds the libevdi handle and the DRM card index |
| `host/evdi/helper_writer.h:8` | linux | writes frames into a named pipe (FIFO) |
| `host/src/capture/helper.rs:18` | evdi | spawns the C evdi_helper process; it must be stopped by pid, never by name, because other tablets run their own |
| `host/src/config/handler.rs:309` | evdi | looks for the evdi_helper binary next to the daemon and in ~/.local/bin |
| `host/src/daemon/session/monitor.rs:409` | linux | pkill -f matches the helper and ffmpeg by command line, because they share a fifo path per slot |

* **Neutral signature.** The helper's contract is already neutral: it writes NV12 frames into a FIFO and reports timing; the
  daemon never calls EVDI itself. A different capture source (a Wayland screencast portal, DXGI on Windows) would be a
  second helper with the same FIFO and CLI.
* **A new platform implements.** A helper that honours `--capture-fifo`, `--card` and the signals listed in
  `host/evdi/helper_signals.h`.
* **Selection.** Compile and configuration time: `FileConfig::helper_path`, found by `find_helper`.
* **Fallback.** No helper binary: the stream does not start and the doctor names the missing file.

### 7. Compositor health (freeze guard)

| Where | Platform | What the code does |
|---|---|---|
| `host/src/daemon/guard/mod.rs:235` | gnome | the stall watchdog runs only on GNOME Wayland |
| `host/src/daemon/guard/probes.rs:5` | gnome | GNOME on Wayland is the only compositor the freeze guard watches |
| `host/src/daemon/guard/probes.rs:15` | gnome | calls org.gnome.Mutter.DisplayConfig.GetCurrentState, which GNOME Shell answers from its main loop |
| `host/src/daemon/guard/probes.rs:32` | gnome | owner of org.gnome.Shell on the session bus, uptime from /proc/<pid>/stat |
| `host/src/daemon/guard/probes.rs:57` | gnome | org.gnome.SessionManager method |
| `host/src/daemon/guard/probes.rs:75` | gnome | waits for GNOME Shell and its session manager; other compositors are not waited for |

* **Neutral signature.** A `CompositorProbe` with `async fn ready()` and `async fn responsive() -> Option<Duration>`. Today the
  GNOME functions are free functions in `daemon/guard/probes.rs`. One implementation, so only the seam is documented.
* **A new platform implements.** A probe that a stalled compositor fails (a D-Bus round trip, a protocol ping).
* **Selection.** Runtime: `is_gnome_wayland()`.
* **Fallback.** No watchdog; nothing is reported.

### 8. KDE on-screen keyboard

| Where | Platform | What the code does |
|---|---|---|
| `host/src/desktop/kde/osk.rs:17` | kde | switches KWin's on-screen keyboard off over D-Bus and restores it afterwards |

* **Neutral signature.** `pub async fn disable()` and `restore()`; exists as `desktop/kde/osk.rs`. Other desktops have no
  module; GNOME and COSMIC would add `desktop/<name>/osk.rs` with the same two functions, and the caller would dispatch.
* **Fallback.** Nothing is done and the on-screen keyboard may appear.

### 9. Service, autostart, tray, paths

| Where | Platform | What the code does |
|---|---|---|
| `Makefile:67` | systemd | reload user units after install |
| `Makefile:77` | gnome | compiles the extension schema |
| `build-all.sh:143` | systemd | restarts the user service after deploying new binaries |
| `host/src/daemon/mod.rs:30` | linux | single-instance check by looking for /proc/<pid> |
| `host/src/daemon/tray.rs:200` | systemd | starts the settings app outside uscreen.service's cgroup so `systemctl stop` does not kill it |
| `host/src/daemon/tray.rs:267` | dbus | StatusNotifierItem tray (ksni); GNOME shows it only with an AppIndicator extension |
| `scripts/uscreen.desktop:2` | linux | freedesktop.org desktop entry |
| `scripts/uscreen.service:3` | systemd | user unit; %h is systemd's home specifier |
| `uscreen-pro-gui/src-tauri/src/adb/install.rs:9` | linux | copies the embedded APK to ~/.local/share/uscreen/app-debug.apk |
| `uscreen-pro-gui/src-tauri/src/config.rs:320` | linux | $HOME; the config lives in ~/.config/uscreen |
| `uscreen-pro-gui/src-tauri/src/daemon.rs:24` | linux | XDG_RUNTIME_DIR |
| `uscreen-pro-gui/src-tauri/src/daemon.rs:43` | linux | pid file under ~/.local/share/uscreen |
| `uscreen-pro-gui/src-tauri/src/daemon.rs:260` | systemd | every start, stop and status of the daemon goes through `systemctl --user` |
| `uscreen-pro-gui/src-tauri/src/daemon.rs:285` | linux | looks in ~/.local/bin first |
| `uscreen-pro-gui/src-tauri/src/daemon.rs:391` | systemd | autostart is the enabled state of uscreen.service |
| `uscreen-pro-gui/src-tauri/src/daemon.rs:402` | systemd | enables or disables uscreen.service for the user |

* **Neutral signature.** A `ServiceManager` with `start`, `stop`, `is_active`, `autostart() -> bool`, `set_autostart(bool)`.
  Today systemd is hard-coded in `src-tauri/src/daemon.rs`, `daemon/tray.rs`, `build-all.sh`, the Makefile and the installer.
  Only one implementation exists, so the seam is documented, not built.
* **A new platform implements.** The five operations for its init system (OpenRC, runit, launchd, Windows services) and a unit
  file equivalent.
* **Selection.** Runtime: `systemctl --user` exists or it does not.
* **Fallback.** The GUI starts the daemon as a plain child process and tells the user that autostart is unavailable.
  (Today it shows the failure of `systemctl`.)
* **Paths.** `~/.config/uscreen`, `~/.local/share/uscreen`, `~/.local/bin` are Linux (XDG) conventions repeated in several
  places. A `paths` module in each crate would own them; on Windows and macOS only that module changes.

### 10. Installer and distribution

| Where | Platform | What the code does |
|---|---|---|
| `scripts/install.sh:200` | systemd | reload user units after installing uscreen.service |
| `scripts/install.sh:223` | gnome | compiles the extension's gsettings schema |
| `scripts/install.sh:275` | uinput | loads the uinput module; the udev rule below makes /dev/uinput usable without root |
| `scripts/lib/distros/arch.sh:7` | arch | package manager, GTK runtime and EVDI strategy of the arch family |
| `scripts/lib/distros/debian.sh:10` | debian | package manager, GTK runtime and EVDI strategy of the debian family |
| `scripts/lib/distros/fedora.sh:8` | fedora | package manager, GTK runtime and EVDI strategy of the fedora family |
| `scripts/lib/distros/suse.sh:6` | suse | package manager, GTK runtime and EVDI strategy of the suse family |
| `scripts/lib/distros/unknown.sh:7` | linux | package manager, GTK runtime and EVDI strategy of the unknown family |
| `scripts/lib/evdi.sh:10` | evdi | getting the EVDI kernel module: packages, DKMS or a source build |
| `scripts/lib/platform.sh:14` | linux | detection of distro family, desktop and session; every `<family>_*` function it calls lives in scripts/lib/distros/ |
| `scripts/lib/platform.sh:44` | linux | family from /etc/os-release ID and ID_LIKE |
| `scripts/lib/platform.sh:58` | linux | image-based distros (Bazzite, Silverblue) need layering and a reboot |
| `scripts/lib/platform.sh:68` | linux | XDG_CURRENT_DESKTOP |
| `scripts/lib/platform.sh:81` | wayland | XDG_SESSION_TYPE, X11 or Wayland |
| `scripts/lib/platform.sh:92` | gnome | the Shell extension needs a log out on GNOME Wayland |
| `scripts/lib/platform.sh:98` | linux | kernel flavour picks the prebuilt EVDI module package on Ubuntu |

* **Neutral signature.** Already a per-family interface, documented in `scripts/lib/distros/_template.sh`:
  `<family>_order`, `_ids`, `_core_packages`, `_evdi_strategy`, `_install_deps`, `_install_gtk`, `_evdi_install`.
* **A new platform implements.** One file in `scripts/lib/distros/` following the template, plus a fixture in
  `scripts/tests/fixtures/os-release.*`.
* **Selection.** Runtime, `us_distro_family` from `/etc/os-release`.
* **Fallback.** `unknown.sh`: copies the files, warns that dependencies must be installed by hand.

### 11. `uscreen doctor`

| Where | Platform | What the code does |
|---|---|---|
| `host/src/cli/doctor/display.rs:5` | linux | if-ladder over desktop backends for the "is the virtual display on" report |
| `host/src/cli/doctor/display.rs:109` | kde | parses `kwin_wayland --version` style banner text |
| `host/src/cli/doctor/display.rs:121` | kde | on-screen keyboard check; says nothing useful off KDE |
| `host/src/cli/doctor/display.rs:240` | systemd | asks systemctl --user whether uscreen.service is enabled |
| `host/src/cli/doctor/system.rs:5` | kde | kscreen-doctor is an optional tool, needed only on KDE |
| `host/src/cli/doctor/system.rs:78` | evdi | reads /sys/devices/evdi/count |
| `host/src/cli/doctor/system.rs:127` | evdi | looks for evdi_helper processes |

* **Neutral signature.** Each check prints through `Report`; the desktop-specific checks call the same dispatch as group 2
  once step 5 exists, instead of their own ladder.
* **Fallback.** A check that does not apply to the desktop says nothing, not "failed".

### 12. Android app and the adb layer

| Where | Platform | What the code does |
|---|---|---|
| `android/app/src/main/AndroidManifest.xml:4` | android | permissions and component declarations |
| `android/app/src/main/java/com/uscreen/Haptics.kt:12` | android | Vibrator and VibrationEffect, which exist from different API levels |
| `android/app/src/main/java/com/uscreen/IdentifyReceiver.kt:17` | android | posts a heads-up notification (needs POST_NOTIFICATIONS on Android 13+) |
| `android/app/src/main/java/com/uscreen/MainActivity.kt:71` | android | show-when-locked and turn-screen-on window flags (API differs by version) |
| `android/app/src/main/java/com/uscreen/MainActivity.kt:97` | android | PowerManager wake lock |
| `android/app/src/main/java/com/uscreen/MainActivity.kt:548` | android | display mode / preferred refresh rate APIs, different before and after Android 11 |
| `android/app/src/main/java/com/uscreen/MainActivity.kt:628` | android | OrientationEventListener |
| `android/app/src/main/java/com/uscreen/MainActivity.kt:670` | android | system bar hiding; WindowInsetsController on newer versions |
| `android/app/src/main/java/com/uscreen/OrientationPolicy.kt:10` | android | Surface rotation constants and panels whose natural orientation is landscape |
| `android/app/src/main/java/com/uscreen/PlatformSupport.kt:18` | android | assumes the Wi-Fi interface is named wlan*, which holds on most but not all devices |
| `android/app/src/main/java/com/uscreen/PlatformSupport.kt:38` | android | Android 8.1 (O_MR1) or later |
| `android/app/src/main/java/com/uscreen/PowerSupport.kt:26` | android | dims by changing the window's own screenBrightness, not the system setting |
| `android/app/src/main/java/com/uscreen/PowerSupport.kt:89` | android | BatteryManager properties |
| `android/app/src/main/java/com/uscreen/PowerSupport.kt:129` | android | Device Admin receiver, needed to lock the screen |
| `android/app/src/main/java/com/uscreen/PowerSupport.kt:133` | android | DevicePolicyManager.lockNow |
| `android/app/src/main/java/com/uscreen/RemoveAdminReceiver.kt:7` | android | Device Admin removal |
| `android/app/src/main/java/com/uscreen/StreamingService.kt:19` | android | foreground service of type connectedDevice (Android 14 and later require the type) |
| `android/app/src/main/java/com/uscreen/TouchCapture.kt:35` | android | a hidden constant of MotionEvent |
| `android/app/src/main/java/com/uscreen/VideoReceiver.kt:160` | android | picks a software decoder by MediaCodecList names |
| `android/app/src/main/java/com/uscreen/VideoReceiver.kt:303` | android | MediaCodec; KEY_LOW_LATENCY needs Android 11 (API 30), and vendor-specific keys cover some Samsung decoders |
| `host/src/net/adb/app.rs:76` | android | parses dumpsys output and a HOME-intent query to tell whether the tablet shows its launcher or lock screen |
| `host/src/net/adb/wifi.rs:3` | android | wireless debugging (adb pair / adb connect) exists on Android 11 and later |
| `host/src/net/installer.rs:161` | linux | looks for app-debug.apk under $HOME/.local/share/uscreen |
| `host/src/net/pairing.rs:66` | android | Android before 11 has no wireless pairing; the fallback is an adb tcpip port allocated per device |
| `host/src/net/pairing.rs:80` | android | `adb tcpip <port>` on an Android 10 or older device |

* **Neutral signature.** Android versions are handled by `Build.VERSION.SDK_INT` branches inside small helpers
  (`PlatformSupport.kt`, `PowerSupport.kt`, `OrientationPolicy.kt`), which is the right seam: callers never test the version.
  The host side of adb (`net/adb/`) is Android-only by nature.
* **A new platform implements.** A new Android version adds a branch in the helper and a tag; another mobile OS would be a new app.
* **Selection.** Runtime, API level.
* **Fallback.** Each helper keeps a pre-API-level path or returns "unsupported" (`lockScreenTouchSupported`).

## Gaps to know about

1. KDE has no module for enabling the output and mapping devices; its code is inline in `capture/display.rs` and `input/mapping.rs`.
2. COSMIC has `enable_output`/`disable_output` only; no mapping, area or scale, and no case in the ladders: it falls into the KDE path.
3. `scale::backend()`, `desktop::detect()` and the per-module `active()` functions each decide the desktop separately, in two different orders.
4. `desktop::session_is_x11()` and `native_gestures_supported()` read the environment on their own.
5. systemd, XDG paths and uinput are assumed everywhere; they are listed in groups 5 and 9 but not abstracted, on purpose, while only one implementation exists.
