# 81: Comprehensive stylus architecture research (Linux, GNOME 46 / Wayland)

Branch `feat/upstream-sync`. **Research only. No implementation code was written and no source file was modified.**
This report is the only change. Implementation waits for review.

Confidence tags used below: **[V]** verified this session from a primary source (spec page, source file, local
machine); **[S]** secondary (search summary, forum, blog) so treat as likely, not certain; **[U]** unverified or from
memory.

## 1. Executive summary

The brief's central idea, "warp the cursor with `wp_pointer_warp_v1` (or an extension) so the browser anchors text
selection", rests on two premises that did not survive checking. **(a)** `wp_pointer_warp_v1` only lets a client move
the pointer *inside its own surface*, with a pointer-enter serial; it cannot move the cursor over another app's
window, and Mutter implements it only from **version 51** (KWin 6.7, Hyprland 0.52.1; not Sway/wlroots), so it is
irrelevant on GNOME 46 and, even where present, useless for a daemon that is not the focused app [V]. **(b)** The
"second cursor" problem is real in Mutter's design (a tablet tool has its own cursor renderer and sprite, and a client
that never binds `zwp_tablet_v2` receives nothing from it) but *the claim that browsers anchor selection at the mouse
cursor is unverified* [V for Mutter side, U for browser side]. Meanwhile the host **already has the "mouse half"**:
`stylus_style = "mouse"` drives the single seat pointer with a uinput *absolute pointer* (`BTN_LEFT` for the tip,
`BTN_RIGHT`/`BTN_MIDDLE` for barrel buttons), which is by construction one cursor shared with the real mouse. The real
conflict is therefore not cursor position but **pressure/tilt vs. mouse semantics**, which live on different devices
that the compositor will not merge. No shipped path (libei `ei_stylus`, GNOME 47+, extension) merges them on GNOME 46:
Ubuntu 24.04 ships libei 1.2.1 and Mutter's EIS client (main branch) offers no stylus device [V]. The best-supported
design is **per-app routing**: both virtual devices exist, and a tiny extension (the repo already ships
`uscreen-app-lock`) reports the focused app so the host sends pen frames to the *tablet* device for art apps and to the
*absolute pointer* for everything else, with an explicit hotkey/override. Barrel buttons are plain `BTN_*`/key
mapping; Air Commands cannot work on this S Pen (no Bluetooth; the SDK reports `UNSUPPORTED_DEVICE`) and must stay a
graceful no-op. Everything marked [U] below, in particular how Firefox/Chromium/Krita behave with each device, needs the
hardware tests in section 8 before any code.

## 2. What is on the machine today (all [V], read-only commands)

| Item | Value |
|---|---|
| OS / shell | Ubuntu 24.04.5, GNOME Shell 46.0, Wayland session (`XDG_SESSION_TYPE=wayland`) |
| Mutter | 46.2-1ubuntu0.24.04.18 |
| libinput | 1.25.0 |
| libei / libeis | **1.2.1** |
| wayland-protocols (headers) | 1.45; `staging/pointer-warp` XML is present |
| xdg-desktop-portal | 1.18.4 (+ `-gnome` 46.2) |
| Mutter binary strings | `zwp_tablet_*_v2`, `zwp_pointer_gesture_*_v1` present; **no `pointer_warp` and no `ei_stylus`** |
| X11 sessions | `ubuntu-xorg.desktop`, `gnome-classic-xorg.desktop` installed |
| `/dev/uinput` | `root:input 0660` with ACL |
| Tools | `ydotool`, `xdotool` installed |
| Installed GNOME extensions | 30+, including the project's own `uscreen-app-lock@uscreen.app` (shell-version 42-47) |

Repo state at HEAD: `host/src/input/uinput.rs` has a pen tablet device (`inject_pen`: pressure, tilt, eraser,
`BTN_TOOL_PEN`) and an absolute pointer device. `pen_as_mouse` (`uinput.rs:1172`) places the absolute pointer and
holds `BTN_LEFT` from pen-down to pen-up; barrel buttons map to right/middle via `stylus_button_primary/secondary`.
`stylus_style` is `"mouse"` (default) or `"tablet"`. The S Pen SDK code described in doc 69 (`android/spen-remote`,
`SamsungSPen.kt`) is **gone at HEAD**: `git ls-files` shows no S Pen file and only an empty `build/` dir remains
(removed by the revert `8e3d3fb`).

## 3. Per-method findings

### Method 1: `wp_pointer_warp_v1`

- **How it works [V]**: staging protocol, one request, `warp_pointer(surface, pointer, x, y, serial)`. `x`,`y` are
  surface-local. Compositors "should" honour it only if the surface has pointer focus (including during an implicit
  grab), reject a wrong enter serial, and reject a position outside the surface. The serial may come from any surface
  of the same client.
- **Absolute position outside the focused surface? No [V].** The target is a position *in the client's own surface*.
  It is a replacement for the old "lock pointer, hint position, unlock" trick for apps like games and Wine's
  `SetCursorPos`, not a global cursor-positioning API.
- **Does it fix text selection? No.** The pen would be clicking in the *browser*, whose surface we do not own. Our
  host daemon / Tauri window is not the focused client and has no pointer-enter serial for the browser surface.
  Even our own focused window could only warp inside itself. It also needs a Wayland client connection with a
  `wl_pointer`, which a headless uinput daemon lacks.
- **Support [V, from the protocol registry page; a search summary claiming KWin 6.4 / Mutter 49 / wlroots 0.19 is
  contradicted by it and is not used]**: Mutter **51**, KWin **6.7**, Hyprland 0.52.1, Jay 1.14.0, Muffin 6.7.4.
  Not listed: Sway, Wayfire, labwc, river, niri, Weston, COSMIC, Mir, GameScope. Staging tier: backward-compatible
  additions allowed, breaking changes need a new major.
- **GNOME 46 [V]**: Mutter 46.2 contains no `pointer_warp` string; the global is not advertised. Ubuntu 24.04 will
  never get Mutter 51 (a stable-release update would not backport a new protocol; [U] on that policy point).
  Ubuntu 26.04 LTS is believed to ship GNOME 50, so it would also lack it [U].
- **Fallback**: none needed, because this method is not part of the recommended design. If used at all, detect the
  global from `wl_registry` and skip when absent.
- **Feasibility**: low value. Needs `wayland-client` in the host and a focused surface. **Rejected for the stated
  goal.**

### Method 2: `libei` (`ei_stylus`, `ei_pointer_absolute`)

- **How it works**: libei is the client side of the EI protocol; the compositor runs the EIS server. A client obtains
  an EIS socket (on GNOME through the RemoteDesktop portal `ConnectToEIS`, wrapped by `liboeffis`), binds
  capabilities the server offers, and sends frames of events.
- **`ei_stylus` [V]**: exists in the libei 1.6.0 API reference. All requests/events "Since Version 1". Capabilities
  as bit flags: ERASE 1, PRESSURE 2, DISTANCE 4, TILT 8, ROTATION 16, AIRBRUSH_FLOW 32. Requests: `proximity_in/out`,
  `tip_down/up`, `erase_start/stop`, `motion(x,y)` (mm for physical, pixels for virtual), `pressure` 0..1, `distance`
  0..1, `tilt(x,y)` ±90°, `rotation` 0..360°, `airbrush_flow`. Buttons go through `ei_button` in the same frame.
  Logical contact requires non-zero pressure if PRESSURE is bound. A tablet's *pad* (buttons/dials) is not covered.
- **Which release added it [S/U]**: a libei maintainer post says "the next release" adds `ei_stylus`; the last
  announcement found was 1.4.1 (April 2025) and a package page shows 1.5.0 with a new "request device" request. So
  most likely 1.5.0, **not confirmed**.
- **Does any compositor expose it? Not found [V for Mutter main]**: Mutter's `meta-eis-client.c` (main) creates only
  "virtual pointer" (pointer+button+scroll), "virtual keyboard", "virtual absolute pointer" (pointer_absolute+button+
  scroll, per viewport, "standalone" or "shared") and "virtual touch screen". No stylus, no gesture devices. KWin /
  wlroots status: **not checked**. Doc 75 also found `ei_gesture_swipe/pinch` in the protocol docs with no confirmed
  compositor implementation; the same caution applies.
- **One device with stylus AND absolute pointer? [U]**: the `ei_stylus` page says a stylus "can be used alone or
  combined with other interfaces on an ei_device" and gives `ei_button` as the example; it says **nothing** about
  `ei_pointer_absolute` or about cursor sharing. Whether the server routes a stylus device to the seat pointer or to a
  tablet-tool object (own cursor, like a physical stylus) is the compositor's choice and undocumented here. Treat
  "shares the cursor" as unknown. A physical stylus in Mutter gets a separate tool cursor (section 3.4), so a
  compositor that maps `ei_stylus` to the same code path would very likely behave the same.
- **GNOME 46 [S/U]**: the RemoteDesktop portal `ConnectToEIS` was merged in Mutter and tracked in libei issue #1;
  the portal method requires RemoteDesktop portal v2. I believe it exists on 46 (portal 1.18.4 installed), but the
  conditions under which 46 offers an *absolute* pointer device (needing a ScreenCast stream attached or not) were not
  established. Each new session also shows a consent dialog unless restore tokens are used. Even in the best case
  GNOME 46 offers **no stylus device**, so `ei_stylus` is unavailable there for certain.
- **Pros**: no `/dev/uinput` permission; proper tablet semantics; sandbox-friendly.
- **Cons**: unavailable on target; portal consent; libei 1.2.1 on Ubuntu 24.04 lacks the stylus interface even
  client-side (would need a newer libei or the pure-Rust `reis` crate; [U] for its stylus coverage).
- **Fallback**: uinput path (current).
- **Feasibility**: medium (Rust: `reis` or `ashpd`); blocked on compositor support. **Watch item, not a plan.**

### Method 3: GNOME Shell extension (D-Bus bridge)

- **CrossMacro [S]**: a macro recorder; its Flathub listing says absolute mouse positioning works on Hyprland, KDE and
  GNOME and that on GNOME a Shell Extension is installed on first run (log out/in may be needed). Its D-Bus API was
  **not read**.
- **`cua` (WinRects) [U]**: no source found; cannot describe it. **`wdotool` [S]**: a GitHub project that describes
  itself as not a drop-in xdotool replacement; its GNOME backend was **not read**. **`ydotool`** (installed here)
  writes to `/dev/uinput` and has no focus awareness [S]; it is just our current mechanism.
- **Mechanism [S]**: extensions run inside gnome-shell and can create a `Clutter.VirtualInputDevice` from the default
  seat and call absolute motion on it (Discourse thread; an extensions.gnome.org reviewer advises Clutter virtual
  devices over `Gdk.Display.warp_pointer`). A virtual pointer in the shell's seat moves the **one** logical pointer,
  anywhere on screen, over any window. Kando's GNOME integration shows the read direction (pointer position and
  focused window over D-Bus; GNOME 45+). The exact 46 API names were not verified [U].
- **Outside the app's own window? Yes in principle**, because the extension runs in the compositor process; this is the
  only mechanism found that can position the shared cursor globally on GNOME 46. But see Method 5: we do not need it
  for positioning, since a uinput absolute pointer already does that. Its real value is **focus information** (the
  focused window's app id / WM_CLASS), which GNOME blocks for ordinary clients (`org.gnome.Shell.Introspect` is
  restricted since 41 [U]).
- **Detection / fallback**: probe the extension's D-Bus name (`NameHasOwner`) at startup and on a timer; absent means
  per-app routing is off and a manual mode toggle is used. Never fail input because the extension is missing.
- **Maintenance risk**: high compared with uinput. Shell extensions break on every GNOME major (JS API churn, ESM
  migration in 45); metadata must list each `shell-version`; installation needs enable plus (on X11 only) restart,
  and on Wayland a **logout/login** for first install; extensions.gnome.org review is slow. Mitigations: the project
  already maintains `uscreen-app-lock` (metadata lists 42-47) so the packaging path exists; keep the new surface tiny
  (one D-Bus signal "focus changed: app id"), no input injection inside the shell.
- **Feasibility**: medium; code is small, but cannot be tested without a real GNOME session.

### Method 4: uinput virtual tablet + separate mouse (current)

- **Why the tablet device is a different "pointer" [V on the Mutter half]**: `meta-wayland-tablet-tool.c` (46.2)
  creates a per-tool `cursor_renderer` (via `meta_backend_get_cursor_renderer_for_device`) and a default xcursor
  sprite; the tool cursor is set by `tool_set_cursor`. `meta_wayland_tablet_tool_handle_event` forwards events only to
  a focused surface and **returns `STOP` even when the client has no tool resource**; tool resources are created only
  for clients that bound the tablet seat. The file shows **no wl_pointer fallback**. So: a client that does not bind
  `zwp_tablet_manager_v2` gets nothing from the tablet device in this file (pointer emulation, if any, would live
  elsewhere: **not checked**).
- **Why selection "fails" [U]**: the brief says browsers anchor selection at the primary mouse cursor position. That
  is plausible given the above (mouse press at the seat pointer position, drag events from the tool), but I found no
  source on Firefox/Chromium behaviour. Related evidence only: a Mozilla bug (1937155) about stylus tab dragging on
  GNOME/Wayland; a CachyOS report that Chromium's tablet support landed in 2025 and clicks can be ignored after
  stylus use on KWin. **This needs the reproduction in section 8 (T1-T3) to pin down.**
- **Any way to bind the two devices? No [V/U]**: Wayland has no device-association concept; Mutter binds tablets to
  *outputs* (gsettings `tablet` mapping) and per-tool settings (`org.gnome.desktop.peripherals.tablet.stylus` strings
  exist in libmutter), not to a mouse. A single uinput device carrying both `ABS_*` pen axes and `REL_*`/pointer
  semantics makes libinput classify it as either a tablet or a pointer; doc 73 already rejected "REL alongside ABS"
  for double movement.
- **X11 [S/U]**: works (xf86-input-libinput maps a tablet tool to the core pointer with XI2 valuators), see Method 6.
- **Verdict**: keep as the pressure path; it cannot also be the text path.

### Method 5: Hybrid dual device with cursor warping

- **Correction of the premise**: warping is not needed to put the *mouse* at the pen. `pen_as_mouse` already emits
  `ABS_X/ABS_Y` on the uinput absolute pointer; libinput feeds the single seat pointer, so the cursor is where the pen
  is, including during hover, and text selection by click-drag works like any mouse. What is missing from that path
  is pressure/tilt, because those only exist on the tablet device.
- **"Mouse to pen position on pen-down" with the tablet device also active**: possible by sending the same pen
  frames to both devices. Expected results [all U, need T4-T7]: art apps that bind the tablet protocol (Krita, GIMP,
  Inkscape via GTK/Qt, Xournal++) receive pressure/tilt; apps that do not (some browsers/Electron) fall back to the
  pointer events. Risks: (1) **duplicate events** in apps that process both (a stroke starting from the pointer
  press and again from the tool down); (2) **two cursors** (tool cursor sprite plus pointer sprite) or the pointer
  hiding while the tool is in proximity; (3) focus differences, since pointer focus and tool focus are tracked
  separately and can sit on different surfaces briefly; (4) pointer acceleration does not apply to absolute devices
  but both must share one coordinate transform or the "jump between devices" appears.
- **Where warping would matter**: only if the pointer device is *relative*. It is not. `wp_pointer_warp_v1` / an
  extension warp would be a patch for a problem that exists only in a design we do not use.
- **Pressure/tilt preserved? Yes via the tablet device, only if the client binds it.**
- **Jump risk**: if one device is updated and the other lags by a frame, the two positions disagree for a frame.
  Emit both in the same loop iteration, same transformed coordinates, tablet first or pointer first consistently.

### Method 6: X11 / XWayland

- **Native X11 session [S/U]**: `xdotool`/XTest and XI2 let a client warp the single core pointer anywhere
  (`XWarpPointer`); uinput devices are picked up by xf86-input-libinput; a tablet tool drives the core pointer and
  also delivers XI2 valuators (pressure/tilt) to the same client, so text selection and pressure coexist. This is the
  one environment where "one cursor, full pen data" is plausible without tricks. Ubuntu 24.04 still ships Xorg
  sessions (installed here [V]). Ubuntu 25.10+ is believed to have removed the GNOME Xorg session [U].
- **XWayland under a Wayland session [U]**: XWayland clients get pointer/tablet events translated from the
  compositor; X clients cannot warp the real cursor outside XWayland windows (Mutter virtualises it), so this is *not*
  a fix. (The XWayland pointer-warp emulation RFC dates from 2016.)
- **Latency [U]**: not measured. Expect X11 and Wayland to be comparable for uinput input; Mutter on Wayland adds one
  compositor frame. Measure with the existing latency tooling if it matters.
- **Verdict**: a legitimate escape hatch for users who need it; not the default direction (X11 is being retired).

## 4. Phase 2: S Pen Air Commands

- **SDK [V, partly]**: `david-allison/S-Pen-Remote-Open-SDK` is a *clean-room* Apache-2.0 implementation of the Samsung
  S Pen Remote SDK, aimed at GPLv3 apps. Samsung's page lists two event kinds: **button** (`ButtonEvent`, press/release
  as `ACTION_DOWN/UP`) and **air motion** (2D relative deltas from the previous position, -1.0..1.0). Our doc 69 notes
  the open SDK exposes no circle gesture; swipes had to be classified from accumulated deltas (the threshold was an
  unverified guess).
- **Support [V]**: requires a Samsung device with the Note-series hardware path (BLE S Pen); "Some Samsung Smart Devices
  do not support S Pen Remote"; `getUnit()` returns null and `connect()` fails with `UNSUPPORTED_DEVICE` otherwise.
  The Tab S9 FE S Pen has no Bluetooth/air actions (user-confirmed by SDK result; a Samsung Community post agrees [S]).
  The SDK's older page lists Note 9/10/20, S22 Ultra, Tab S6/S7/S8 as supported; the S9 FE is not listed.
  Air motion drains the pen battery, so listeners should be registered only when a mapping exists.
- **Foreground requirement: not documented [U]**. Samsung's page says only to `connect()` and `disconnect()` on app
  start/stop. Whether events arrive while the app is not foreground (tablet used as plain display) is unknown and
  must be tested on a pen that supports it; this user's pen cannot test it.
- **Transport design**: a separate WebSocket message type (e.g. `pen_remote{kind: button|air, ...}`), not mixed into
  motion frames, so the host can drop it when no mapping exists. The host maps it through the existing keyboard path
  (`run_action` / `UScreen Keyboard`).
- **Non-Samsung / unsupported**: `isSupportedDevice()` false or `UNSUPPORTED_DEVICE` -> do not register, hide the
  option, log once. No-op, never an error.
- **Distinguish from the pen's side buttons**: the **barrel buttons are not S Pen Remote**. They arrive as
  `MotionEvent.buttonState` (`BUTTON_STYLUS_PRIMARY/SECONDARY`) on every S Pen, including this one. That is the
  working path for the user today.

## 5. Comparison table

"Barrel buttons" = ability to map primary/secondary pen buttons. "Text selection" refers to the behaviour the user
reported; **[U] = not tested.**

| Method | Text selection | Pressure/Tilt | Barrel buttons | Air Commands | GNOME 46 | GNOME 47+ | KDE | X11 | Maintenance burden | Feasibility |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| 1 `wp_pointer_warp_v1` | **No** (own-surface only, needs focus + serial) | No | No | No | **Not available** (no global) | Mutter **51** only (via the registry) | KWin **6.7** | n/a | Low | **Rejected** (cannot do the job) |
| 2 libei `ei_stylus` | [U] (depends on cursor mapping) | Yes (all six capabilities) | Yes via `ei_button` | No | **Not available** (libei 1.2.1, no stylus device) | Not found in Mutter main EIS client | [U] not checked | n/a | Medium (portal, consent) | Blocked; watch |
| 3 Shell extension | Positioning yes; injection not recommended | Not by itself | Not by itself | No | Likely (API names [U]) | Needs port per major | n/a (KWin script is a separate thing) | n/a | **High** (every GNOME release) | Medium; best used only for focus info |
| 4 uinput tablet + mouse (current) | Mouse device: **yes**. Tablet device: [U] (the reported bug) | Tablet device: yes | Yes (`BTN_*`) | Host key path | Works | Works | Works [U] | Works [S] | Low | Done; two separate styles |
| 5 Hybrid dual device (no warp) | Pointer half: yes; duplicate/cursor issues [U] | Yes if the client binds tablet protocol | Yes | Host key path | Works | Works | [U] | n/a | Low-medium | Feasible; needs tests T4-T7 |
| 5b Hybrid + per-app routing | Yes for non-art apps (pointer) | Yes for art apps (tablet) | Yes | Host key path | Needs extension for focus | Extension port | [U] | n/a | Medium (extension) | **Recommended candidate** |
| 6 X11 session | Yes [S] | Yes [S] | Yes | Host key path | Xorg session installed | Xorg session removed in later releases [U] | Plasma X11 [U] | Native | Low | Escape hatch only |

## 6. Proposed architecture

Principle: **no cursor warping; two devices; route per app; detect by capability, not version.**

1. **GNOME 46 backend (now)**: keep the two uinput devices. Add a third mode, `stylus_style = "auto"`, only after the
   tests pass:
   - tablet device receives pen frames (pressure, tilt, eraser, proximity) when the focused app is on an "art apps"
     list (Krita, GIMP, Inkscape, Blender, Xournal++, Rnote, etc., user-editable);
   - absolute-pointer device receives the same frames as mouse input otherwise (browsers, terminals, IDEs);
   - a hotkey / on-tablet toggle forces either side; the last choice is remembered per app.
   - Focus comes from a tiny extension (extend `uscreen-app-lock` or a sibling) that emits `FocusedAppChanged(app_id)`
     over D-Bus. If it is not installed the host stays on the user's fixed style (today's behaviour).
2. **GNOME 47+ and later**: same backend. Reassess when (a) Mutter's EIS client offers a stylus device and
   (b) a newer libei is in the target distro. Then add a libei backend behind the same trait and prefer it where the
   compositor actually shares the cursor. `wp_pointer_warp_v1` stays unused.
3. **Compositor / backend selection**: at start read `XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP`, and
   `org.gnome.Shell` `ShellVersion` over D-Bus; list Wayland globals via `wl_registry` (look for `zwp_tablet_manager_v2`,
   `wp_pointer_warp_v1`) and EIS stylus availability via the portal. Choose by **capability**, log the decision, and
   fall back to the uinput devices. Surface the result in the existing `doctor` output (`host/src/doctor.rs`).
4. **S Pen buttons and air commands**: barrel buttons stay `buttonState`-diff based (doc 73 design) and map to
   right/middle click or a key combination through `stylus_button_*`. Air commands are a separate optional message
   type, registered only when `isSupportedDevice()`; on this tablet nothing is registered.
5. **Text-selection bug**: solve by *using the pointer device* when the target app does not use the tablet protocol,
   not by warping. If tests show the pointer path alone still anchors wrongly, the fallback is a short "pointer
   assist": before tablet pen-down, send one pointer move to the same coordinates (no button) so the seat pointer is
   already under the pen.
6. **Gains and losses**:
   | Scenario | Gains | Loses |
   |---|---|---|
   | GNOME 46 + extension | Pressure/tilt in art apps, mouse precision elsewhere, barrel buttons | Extra install + logout; risk of duplicate events |
   | GNOME 46, no extension | Today's behaviour (pick one style) | No automatic routing |
   | GNOME 47+ with libei stylus (future) | Single native device, no `/dev/uinput` | Portal consent, newer libei |
   | KDE Plasma 6 | Same uinput path; warp available from 6.7 but unneeded | Extension for focus not applicable; use KWin script [U] |
   | X11 session | One cursor with pressure | Session being retired |

## 7. Risks and unknowns

**Unverified claims (must not be built on until tested):**
- That browsers anchor selection at the mouse cursor; how Firefox/Chromium/Electron/Krita bind `zwp_tablet_v2` today.
- Whether Mutter 46 emulates wl_pointer events from a tablet tool for clients without tablet support (not in
  `meta-wayland-tablet-tool.c`; other files not read).
- Which libei release added `ei_stylus`; whether KWin or wlroots compositors implement it; whether it shares the cursor.
- Whether GNOME 46 `ConnectToEIS` offers an absolute pointer without a ScreenCast stream.
- CrossMacro's and `cua`'s D-Bus APIs; `wdotool`'s GNOME backend; exact Clutter virtual device API on Shell 46.
- Whether `org.gnome.Shell.Introspect` is blocked for our process (believed yes).
- S Pen Remote events in the background; real air-motion scale on supported pens.
- Ubuntu 26.04 GNOME version; Xorg session removal dates.
- Doc 75's libinput gesture work and doc 73's input engine remain **hardware-unverified**.

**Hardware to test:** Tab S9 FE with its S Pen (barrel buttons, pressure, tilt, hover); the GNOME 46 laptop; ideally a
KDE Plasma 6 machine and a Note-series/BLE S Pen owner for air commands; a physical mouse for the "primary mouse"
comparison.

**What could break on update:** Mutter changing tablet-tool cursor/focus behaviour; GNOME extension API changes each
major; Chromium/Firefox changing tablet-protocol support (it changed in 2025 per the CachyOS report); libinput
tablet-tool behaviour; `/dev/uinput` permissions on a newer Ubuntu; the portal's EIS offering stylus devices (would
change the best design).

**Worst case:** keep `stylus_style = mouse|tablet` as a manual toggle, document that pressure and text selection are
exclusive on GNOME 46 Wayland, and offer the Xorg session for users who need both. This is what exists today.

## 8. Test plan to run before any implementation (needs the real pen)

| # | Test | Learns |
|---|---|---|
| T1 | `WAYLAND_DEBUG=1 firefox`/chromium: does the client bind `zwp_tablet_manager_v2`? | whether browsers see the tablet device at all |
| T2 | Tablet-style pen in Firefox: click, drag-select text, log events | reproduces/refutes the text-selection claim |
| T3 | `libinput debug-events` + `libinput list-devices` on both virtual devices | classification of each device |
| T4 | Both devices fed from the same frames; Krita stroke | duplicate stroke start? |
| T5 | Same, watch the screen | one cursor or two; does the pointer hide? |
| T6 | Same in Firefox/GTK text field | selection anchor with both active |
| T7 | Rapid pen-down/up while moving | jump between devices |
| T8 | Install a minimal focus-reporting extension | latency and reliability of app-id signals on 46 |
| T9 | `wayland-info`/registry dump | confirm no `wp_pointer_warp_v1` on this machine |
| T10 | Physical S Pen buttons: `buttonState` log | confirms the barrel path is independent of the SDK |

## 9. Sources

- https://wayland.app/protocols/pointer-warp-v1 (read)
- https://libinput.pages.freedesktop.org/libei/ and `.../interfaces/` and `.../interfaces/ei_stylus/index.html` (read)
- https://raw.githubusercontent.com/GNOME/mutter/main/src/backends/meta-eis-client.c (read)
- https://raw.githubusercontent.com/GNOME/mutter/46.2/src/wayland/meta-wayland-tablet-tool.c (read)
- https://developer.samsung.com/galaxy-spen-remote/s-pen-remote-sdk.html (read)
- https://github.com/david-allison/S-Pen-Remote-Open-SDK (README only)
- Search-result only, not opened: Phoronix Wine pointer-warp article; KWin MR 6460; libei tracking issue #1; Mutter MR
  3228; Flathub CrossMacro listing; wdotool repo; Kando gnome-shell-integration; Mozilla bug 1937155; CachyOS Chromium
  stylus thread; Samsung Community S9 FE thread.
- Repo: docs 69, 73, 75; `host/src/input/uinput.rs`; `host/src/input/events.rs`; `extensions/uscreen-app-lock`.
