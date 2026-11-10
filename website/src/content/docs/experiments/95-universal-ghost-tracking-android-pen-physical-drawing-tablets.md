---
title: "95 Universal ghost tracking (Android pen + physical drawing tablets)"
sidebar:
  order: 54
  label: "95 Universal ghost tracking (Android pen + physical drawing "
---

Branch `feat/upstream-sync`. Phase 1 (research) was written **before** any code. Phases 2-3 and the test results are
appended at the bottom as they land.

| Field | Value |
| :-- | :-- |
| **ID** | 95 |
| **Date** | not recorded |
| **Build** | `f3866a3-dirty` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

Tags: **[V]** verified this session from a primary source or on this machine; **[S]** secondary source; **[U]**
unverified, needs the hardware test named next to it.

Test machine: Ubuntu 24.04, GNOME Shell 46.0, Mutter 46.2, libinput 1.25.0, Wayland. Physical tablet on the USB bus:
`28bd:0931` "UGTABLET 6 inch PenTablet" (an XP-Pen, `lsusb` says "XP-Pen 6 inch PenTablet").

### Why the cursor vanishes (root cause) [V]

Mutter 46.2 `src/backends/meta-backend.c`, `update_pointer_visibility_from_event()`, called for **every** event
(proximity included), switches on the source device class:

| Source device | Effect on the regular pointer |
|---|---|
| touchscreen | hide |
| pointer / touchpad | record `last_pointer_motion = now`, **show** |
| tablet / pen / eraser / cursor | **Wayland only**: hide if `now > last_pointer_motion + 300 ms` (`HIDDEN_POINTER_TIMEOUT`) |
| keyboard, pad, ... | nothing |

So a pen on the tablet device alone always ends with the seat pointer hidden, and the tool's own cursor is only drawn
for clients that bind `zwp_tablet_v2` and set one (Chrome does not draw one on GNOME/Wayland: the "invisible cursor").
`pen_as_mouse` ("mouse" style) avoids it but loses pressure/tilt.

**Ghost tracking follows directly:** every pen sample is *also* written as a pointer motion on the absolute pointer.
Pointer events refresh `last_pointer_motion` at the pen's sample rate, so tablet events arriving within 300 ms never hide
it. The pointer stays visible for as long as samples keep coming (hover included) and X11 needs none of this (section 6).

### Reading a physical tablet in Rust (`evdev` crate)

`evdev` 0.13.2 (docs.rs, [V]); crates.io is reachable from this machine [V].

- Enumerate: `evdev::enumerate()` yields `(PathBuf, Device)` for every `/dev/input/event*`; or `Device::open(path)`.
- Identify (what the code matches, in order):
  1. skip anything named `UScreen*` or with vendor `0x4553` (our own virtual devices);
  2. `supported_absolute_axes()` contains `ABS_X`, `ABS_Y` **and** `ABS_PRESSURE`;
  3. `supported_keys()` contains `BTN_TOOL_PEN` **and** `BTN_TOUCH`;
  4. optional config override `physical_tablet_device = "28bd:0931"` (matches `input_id()` vendor:product).
  `ABS_TILT_X/Y`, `BTN_STYLUS`, `BTN_STYLUS2`, `BTN_TOOL_RUBBER` are optional extras.
- Why not trust `ID_INPUT_TABLET`: a tablet exposes several nodes. This one is three USB interfaces: `event21`
  "…Mouse" (REL, a pad-as-mouse), `event22` "…Keyboard" (express keys) and **`event23` "UGTABLET 6 inch PenTablet"** (the pen).
  Only capability matching picks event23. [V, `/proc/bus/input/devices`]
- Read: `Device::fetch_events()` (handles `SYN_DROPPED` resync) in a dedicated thread with the fd non-blocking +
  `poll(2)` timeout so shutdown, unplug (`ENODEV`) and config toggles are noticed. `into_event_stream()` needs the crate's
  `tokio` feature; a plain thread is simpler because the injector is blocking and mutex-guarded anyway.
- Grab: `Device::grab()` = `EVIOCGRAB`: "exclusive access, other clients including kernel-internal ones stop receiving the
  device's events" (docs.rs [V]). It works against readers that already hold the node open (Mutter), is undone by the kernel
  when our fd closes, and works on X11, GNOME, KDE and wlroots alike.

**This machine's pen, measured with read-only ioctls on event23 [V]:**

| Capability | Value |
|---|---|
| id | bus 0003, `28bd:0931`, version 0100 |
| `ABS_X` / `ABS_Y` | 0..32767, resolution 205 / 328 units/mm (= 160 x 100 mm, matches udev `ID_INPUT_WIDTH_MM=159`) |
| `ABS_PRESSURE` | 0..8191 (13 bit), no resolution |
| `ABS_TILT_X/Y` | -127..127, **resolution 0**, so raw HID units, not degrees [U: degrees per unit; code assumes ±127 = ±60°, XP-Pen's spec] |
| `ABS_DISTANCE` | **absent**, so hover is only visible as `BTN_TOOL_PEN` |
| keys | `BTN_TOOL_PEN`, `BTN_TOOL_RUBBER`, `BTN_TOUCH`, `BTN_STYLUS` (no `BTN_STYLUS2`) |

Event semantics (Linux multitouch-less tablet protocol, [S] kernel docs): `BTN_TOOL_PEN`=1 is proximity-in, =0
proximity-out; `BTN_TOUCH` is the tip; `ABS_*` carry the sample; `SYN_REPORT` closes the frame. Hover samples are
frames with `BTN_TOOL_PEN`=1 and `BTN_TOUCH`=0. `BTN_TOOL_RUBBER` is the eraser end.

### Permissions without root

- Node permissions here: `/dev/input/event23` is `root:input 0660`, **no ACL** [V `getfacl`]. It is readable today only
  because the user is in the `input` group [V `groups`]. `ID_INPUT_TABLET=1` is set but systemd's `70-uaccess.rules` does
  not grant tablets a `uaccess` ACL [V: rule file present, node has no ACL].
- `/dev/uinput` is `root:input 0660` with an ACL [V]; the repo already ships `packaging/60-uscreen-uinput.rules` (`uaccess`).
- Rule for the tablet node (new, `packaging/60-uscreen-tablet-access.rules`; must sort **before** `73-seat-late.rules`
  where uaccess ACLs are applied):

  ```
  SUBSYSTEM=="input", KERNEL=="event[0-9]*", ENV{ID_INPUT_TABLET}=="1", ATTRS{idVendor}=="28bd", ATTRS{idProduct}=="0931", TAG+="uaccess"
  ```
  `uaccess` hands the node to whoever is logged in at the seat, no `input` group membership. OpenTabletDriver does the same
  for `hidraw*` and `usb` nodes with `TAG+="uaccess", TAG+="udev-acl"` [V, its `generate-rules.sh`]. Reload with
  `udevadm control --reload && udevadm trigger --subsystem-match=input --action=change`.

### The OpenTabletDriver trick (hide from the compositor)

OpenTabletDriver's `generate-rules.sh` [V, read from the repo] emits, per tablet that needs it, one line:

```
SUBSYSTEM=="input", ATTRS{idVendor}=="<vid>", ATTRS{idProduct}=="<pid>", ENV{LIBINPUT_IGNORE_DEVICE}="<n>"
```
(value from a per-tablet `libinputoverride` attribute, only written when > 0; vid/pid are 4-digit lowercase hex).

libinput docs [V, `ignoring-devices.html`]: any value other than `"0"` makes libinput skip the device entirely (never
appears; `libinput_path_add_device` fails). The doc's own example matches `ENV{ID_VENDOR_ID}`/`ENV{ID_MODEL_ID}` on
`KERNEL=="event[0-9]*"` with `ACTION!="remove"`, saved as `/etc/udev/rules.d/99-ignore-my-device.rules`. Crucially it
"signals that only libinput should ignore this device but other parts of the stack should continue treating this device
normally": **our evdev reader is unaffected**.

Rule used here, one line per event node of the tablet (`packaging/99-uscreen-hide-tablet.rules.in`, generated for the
user's IDs by `uscreen tablet-rules --vid 28bd --pid 0931`):

```
ACTION!="remove", SUBSYSTEM=="input", KERNEL=="event[0-9]*", ENV{ID_VENDOR_ID}=="28bd", ENV{ID_MODEL_ID}=="0931", ENV{LIBINPUT_IGNORE_DEVICE}="1"
```
Note that `ID_VENDOR_ID` here is `28bd` (udev shows `ID_VENDOR_ID=28bd`, `ID_MODEL_ID=0931` [V]). It hides all three event
nodes of the tablet, intended (the express keys would otherwise still act as a keyboard; see Risks).

**Hazard (design consequence):** with this rule installed and uscreen not running, the tablet is dead for the whole desktop.
So the **default** hiding method is `EVIOCGRAB` while the daemon has physical-tablet support on (self-undoing); the udev rule
is the optional persistent method, installed by the user explicitly (it needs root). Grab has one weakness [U]: a
compositor that already saw proximity-in before our grab can be left with a stuck tool; the udev rule has no such race.

### Persistent cursor (default) [V mechanism / U on screen]

Implemented as section 0 says: every pen sample (hover, move, down, up) also goes to `UScreen Pointer` as `ABS_X/ABS_Y`,
so hover keeps the pointer visible like a mouse. Pen out of range stops the stream; the cursor simply stays where it was.
Both devices must receive the *same transformed coordinates* in the same loop iteration (doc 81 risk: a one-frame jump
between devices). Pointer coordinates go through the existing `desktop_xy()` (pointer area on the desktop).

### Optional idle hide

Premise in the brief: "stop updating so the compositor's idle timer hides it". **Not true on Mutter 46.2 [V]**: there is no idle
hiding; the pointer hides only on touchscreen input, on tablet-class input >300 ms after the last pointer motion (Wayland),
and on typing (separate code). So a silent pointer stays visible forever.

Mechanism used (uses exactly the table in section 0): after `stylus_idle_hide_timeout` seconds with the pen **out of range**
and no events, emit one tablet-class frame on `UScreen Pen` (proximity-in + position, then proximity-out) with **no** pointer
motion. By then `now > last_pointer_motion + 300 ms`, so Mutter hides the pointer; the next pen sample re-shows it
(pointer-class event). `0` (default) disables it. Detecting "out of range": Android `ACTION_HOVER_EXIT` (pen action 4) and
physical `BTN_TOOL_PEN`=0. Not hover = no timer reset; any new sample resets it. Hide stays in effect until the next pen or mouse event. KWin and wlroots have no
equivalent code path that I verified [U]; on those the timeout is a no-op rather than a wrong action (fallback "park the
pointer in a corner" was considered and rejected: a stray cursor at a corner is not hidden).

### Portability

| Environment | Ghost tracking needed? | Notes |
|---|---|---|
| GNOME Wayland | **yes** [V, section 0] | verified in the Mutter 46.2 source; older Mutter versions not checked [U] |
| KDE Plasma Wayland | likely yes [U] | KWin tablet tool cursor behaviour not read; dual injection is harmless there (one seat pointer + a tool) |
| X11 (Xorg) | **no** [S] | xf86-input-libinput maps the tablet to the core pointer: one cursor; dual injection would only double motion, so the code skips the mouse half when `XDG_SESSION_TYPE=x11` |
| XWayland apps | n/a | inherit the Wayland session behaviour |

### Routing, focus and the hotkey (design)

- **Smart click routing**: art app focused → tip down/up/pressure/tilt to `UScreen Pen`, pointer gets motion only; other app
  → `BTN_LEFT` (+ side buttons) to `UScreen Pointer`, pen device gets motion/hover only (no `BTN_TOUCH`).
- **Focused-app source** (no client API on GNOME Wayland [S, doc 81]): the existing `uscreen-app-lock` Shell extension is
  extended to write `~/.local/share/uscreen/focus.json` on every focus change; X11 sessions read `_NET_ACTIVE_WINDOW` +
  `WM_CLASS` through `x11rb` (already a dependency); KDE Wayland has no source yet [U] → falls back to the default route.
- **Hotkey Ctrl+Alt+P**: a compositor-global key cannot be grabbed by a daemon on Wayland. The Shell extension owns the
  keybinding and bumps a counter in `focus.json`; the host toggles "force tablet" for the current app. Elsewhere,
  `uscreen toggle-tablet` (CLI) does the same and can be bound to any key.
- `stylus_art_apps` default (substring match on app id, desktop file or window class, user-editable in `config.toml`): krita, gimp, inkscape, blender, xournalpp, rnote, mypaint, aseprite, pinta, darktable, freecad, onlyoffice.

### Risks / unknowns carried into testing

1. Whether Chrome/Krita get a second visible cursor when both devices are active (tool cursor + pointer) [U].
2. Mutter hide-by-blip: whether a proximity-in/out pair makes a one-frame tool cursor flash [U].
3. Tilt unit scaling for the XP-Pen (±127 raw -> degrees) [U].
4. Grab race at hotplug (section 3) [U].
5. Extension keybinding needs a Shell restart / re-login on Wayland to load [U].
6. Tablet express keys (`event22`) are not handled by this work; ignoring the whole tablet with the udev rule disables them.
7. KDE: no focus source and no idle-hide mechanism implemented [U].

Sources: Mutter 46.2 `meta-backend.c` (gitlab raw); mutter commit f9ebbaf39258 (tablet hides pointer); libinput docs
`ignoring-devices`, `device-configuration-via-udev`, `tablet-support`; OpenTabletDriver `generate-rules.sh` and Required Permissions
wiki; docs.rs `evdev` 0.13.2 `Device`; local `/proc/bus/input/devices`, `udevadm info`, `getfacl`, EVIOCG* ioctls.

---


## Result

### Implementation (Phases 2 and 3)

| Piece | Where |
|---|---|
| Ghost injection, routing, focus tracking, idle hide (+ 11 unit tests) | `host/src/input/ghost.rs` |
| One shared pen path for Android and physical (`apply_pen`) | `host/src/input/events.rs` |
| evdev reader, event state machine, device selection, udev rule generator (+ 7 unit tests) | `host/src/input/physical.rs` |
| New settings: `stylus_style = "auto"` (new default), `stylus_idle_hide_timeout`, `stylus_art_apps`, `physical_tablet_support` (default **false**), `physical_tablet_device`, `physical_tablet_grab` | `host/src/config.rs`, `host/src/options.rs`, `uscreen-pro-gui/src-tauri/src/config.rs` |
| GUI: "Pen acts as: Auto", "Physical tablet support" toggle, "Hide idle pen cursor" | `uscreen-pro-gui/src/views/GlobalSettings.tsx`, `components/PowerSettings.tsx`, `lib/options.ts` |
| Android: `auto` style | `android/.../HostLink.kt`, `Prefs.kt`, `ui/SettingsSheet.kt` |
| Focus + Ctrl+Alt+P | `extensions/uscreen-app-lock` (v3, adds a GSettings schema) and `uscreen toggle-tablet` |
| udev rules | `uscreen tablet-rules [--vid --pid] [--hide]`; examples for this XP-Pen in `packaging/tablet/` (validated with `udevadm verify`; **not installed by any package or script**) |

Decisions worth knowing:

* `auto` is skipped on X11 (section 6) and when the absolute pointer does not exist (`pointer_handoff = false`); both fall back to the
  old plain-pen path.
* The tip's target is fixed at pen-down (`Tip`), so flipping the route mid-stroke can never leave `BTN_LEFT` or a tablet contact stuck.
* Per-app route overrides (Ctrl+Alt+P) live in memory; they reset when the daemon restarts.
* The physical tablet feeds the **same** `UScreen Pen`/`UScreen Pointer` as the Android tablet, so it addresses whichever output
  those devices are mapped to in the current input mode (Stylus mode = the laptop's main screen; Display mode = the virtual
  display). Use Stylus mode to draw on the main screen with the physical pen.
* The grab is released and the virtual devices are reset when the tablet is unplugged or the toggle is switched off.
* Physical tablet samples do not count as tablet activity for the Android idle-dim timer.

---

### Verification (2026-11-08, this machine: GNOME 46 Wayland, SM-X616B, XP-Pen 28bd:0931)

Build: `./build-all.sh` succeeded (APK, EVDI helper, release daemon, Tauri GUI, forced deploy to `~/.local/bin`, `uscreen.service`
restarted); APK `f3866a3-dirty` installed on the tablet with `adb install -r` (`Success`).
Tests: host 120 passed (incl. 18 new), Android `HostLinkTest` passed, Tauri crate 8 passed, `tsc --noEmit` clean, `udevadm verify` 2/2.

**Done by me, on the real daemon and real kernel devices** (`libinput debug-events` on `UScreen Pen` + `UScreen Pointer`):

| Test | Result |
|---|---|
| Android path: virtual HID pen on the tablet (`scripts/tablet_inject.py pen-hover`, hover only) | 40 hover samples -> 40 `POINTER_MOTION_ABSOLUTE` + 39 `TABLET_TOOL_AXIS` (+1 proximity-in), same timestamps; proximity in/out seen. Both devices follow the pen. |
| Idle hide, 3 s, Android | `Pen out of range for 3s - cursor hidden` logged; one extra proximity in/out pair on `UScreen Pen`, **no** pointer motion during it |
| Physical path: throwaway uinput pen `dead:beef` (same axes/keys as the XP-Pen) selected via `physical_tablet_device` | found by capabilities, grabbed (`EVIOCGRAB`), 30 samples -> 30 pointer + 29 tablet events, idle hide after 3 s, released on unplug |
| Real XP-Pen | detected as `/dev/input/event23`; grabbed and released cleanly within ~1 s of toggling support on/off |

A first idle-hide run failed because my test edit created a duplicate TOML key (the daemon had re-saved the config with the new
default keys); a second run with a valid edit passed. Not a product bug, but note the daemon rewrites `config.toml` with the new keys.

**NOT done / cannot be done by me; needs you with a pen in hand:**

1. Seeing the cursor: I cannot see the screen. That the cursor *stays visible* while hovering in Chrome, and *disappears* after the
   timeout, rests on the Mutter source (section 0) and on the event streams above, not on a screenshot.
2. Pen contact: tip down/up routing (Chrome click + drag-select, Krita pressure) is covered by unit tests with files standing in for
   uinput. I did not click on your live desktop. The focus-based routing needs the extension loaded (below).
3. A physical pen touching/hovering the real XP-Pen (pressure scale 8191 -> 4096 and tilt ±127 -> ±60° are assumptions, section 1).
4. The GNOME extension (v3: focus reporting + Ctrl+Alt+P) is syntax- and schema-checked only. A Wayland session must **log out/in**
   to load new extension code. Install with `make install` or `scripts/install.sh` (both copy the extension and its schema). Until then focus is unknown, the route
   defaults to *mouse* for every app, and `uscreen toggle-tablet` (bind it to a key) flips it.
5. Your tablet profile `[tablets.DEVICE_SERIAL]` pins `stylus_style = "tablet"`; I restored it after my tests. Switch it to **Auto**
   in the GUI (Device settings -> Pen acts as) or the feature is not active for that tablet. New installs default to Auto.
6. KDE Plasma: dual injection is expected to be harmless but untested; no focus source and no idle-hide mechanism there.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/95_universal_ghost_tracking.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/95_universal_ghost_tracking.md)
