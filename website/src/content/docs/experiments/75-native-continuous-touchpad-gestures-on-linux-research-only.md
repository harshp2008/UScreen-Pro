---
title: "75 Native, continuous touchpad gestures on Linux (research only)"
sidebar:
  order: 49
  label: "75 Native, continuous touchpad gestures on Linux (research o"
---

Branch `feat/upstream-sync`. **No implementation code was written and no source file was modified.** This report is
the only change. Implementation waits for review.

| Field | Value |
| :-- | :-- |
| **ID** | 75 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Executive summary

Continuous, 1:1 gestures on Linux are not made by moving a cursor with a button held, and not by `Ctrl` + wheel.
Compositors (GNOME Shell/Mutter, KWin) and applications consume **libinput's `GESTURE_SWIPE_*` / `GESTURE_PINCH_*`
events**: per-frame `dx`/`dy`, an absolute pinch `scale`, and a rotation delta. libinput has **no API for injecting
those events**. It *derives* them from a device that looks like a real multi-touch touchpad. So the only
compositor-agnostic, Wayland-safe way to get native continuous gestures is the one this host already half-has: a
**uinput virtual multi-touch touchpad** (what `inputtino`'s `trackpad.cpp` builds). The tablet sends continuous
deltas, the host replays them as N synthetic fingers moving on that pad, and libinput + the compositor do the
rest: workspace dragging follows the fingers and snaps on release (GNOME does that itself), and apps get native
pinch. This also fixes failure 3 by construction: the compositor, not our code, decides what 3 or 4 fingers mean.
Caveats: it only works where the compositor reads libinput gestures (GNOME and KDE on Wayland; **not** X11 or
Cinnamon/Muffin), libinput adds its own thresholds before a gesture starts, and none of this has been tried on real
hardware by me. A newer EIS/libei gesture protocol exists on paper (not confirmed shipped or implemented) and is
not a basis for a design today.

### What the premise gets wrong (important)

The brief's Phase 2 says a workspace drag needs "a cursor moved with a button held" and that native pinch needs
`Ctrl` + scroll. Neither is how the desktop does it:

- GNOME's workspace swipe never looks at the pointer. It listens for `Clutter.EventType.TOUCHPAD_SWIPE` (the
  Mutter wrapper of libinput's swipe events). Moving the cursor with `BTN_LEFT` held would drag a *window or
  selection*, not the workspace.
- `Ctrl` + wheel is only the fallback that apps and X11 toolkits use when no pinch event exists. Wayland apps get
  real pinch via `zwp_pointer_gestures_v1`, fed from libinput.

Also, the code at HEAD (`243a3a9`) still sends `gesture` messages and replays them as keys
(`host/src/input/events.rs:367-372`: `pinch` -> hold Ctrl + wheel, `workspace_prev/next` -> `Meta+PageUp/PageDown`).
Doc 71 had already built a "UScreen Gestures" virtual multi-touch touchpad for pinch; it was deleted with the
rest of the v1 revert (docs 72/73), so this is not new ground, it is the removed approach extended to swipe.

Host used for this research: Ubuntu, GNOME on Wayland, libinput 1.25.0, `/dev/uinput` is `root:input 0660` (ACL
present). The host crate talks to uinput through raw `libc` ioctls (`host/src/input/uinput.rs`); it does not use the
`evdev` crate.


## Result

### Phase 1: findings per library



### libinput

Read: the gestures page and the `event_gesture` API reference.

- Events: `LIBINPUT_EVENT_GESTURE_{SWIPE,PINCH,HOLD}_{BEGIN,UPDATE,END}`. A sequence always starts with BEGIN.
- **Swipe**: 3 or more fingers; each UPDATE carries `dx`/`dy` of the fingers' logical centre, *relative to the
  previous event* (`get_dx`, accelerated, or `get_dx_unaccelerated`; both normalised to a 1000 dpi device).
- **Pinch**: 2 or more fingers. UPDATE carries the centre `dx`/`dy`, `get_angle_delta` (degrees since the previous
  update, clockwise positive) and `get_scale`, which is **absolute**: current finger distance / distance at
  gesture start (BEGIN = 1.0, END = last scale).
- Finger count is fixed for a gesture's life; a new finger cancels it (`get_cancelled` at END) and may start a new
  one. With two-slot pads, 3 fingers can look like a swipe or a pinch; libinput assumes a swipe.
- libinput interprets gestures **only for touchpads** (indirect touch). For touchscreens it passes raw touch points.
  Fingers in software button areas are ignored.
- **Injection**: libinput is a consumer. It has no "emit gesture" call, and neither `evdev` nor `uinput` has a
  gesture event type (the kernel only has `ABS_MT_*`, `BTN_TOOL_*TAP`, `BTN_TOUCH`). Therefore the **only way to make
  libinput emit swipe/pinch is to feed it touchpad-shaped evdev input** and let its state machine recognise the
  gesture. Consequences: libinput owns the thresholds, it may reinterpret a 2-finger motion as scroll rather than
  pinch, tap-to-click and (libinput >= 1.28) **three-finger drag** (user-configurable) can claim 3-finger input
  before it becomes a swipe, and the first few millimetres are consumed before BEGIN.
- Device requirements (touchpads page): slots for pure MT, or `BTN_TOOL_{FINGER,DOUBLETAP,TRIPLETAP,QUADTAP,QUINTTAP}`
  for single-touch; `INPUT_PROP_BUTTONPAD` marks a clickpad; libinput ignores the `BTN_TOOL_*` codes when slots are
  tracked correctly. Resolution on the ABS axes matters because libinput normalises motion by it.
  **Not verified**: the exact timeouts/thresholds libinput uses to pick swipe vs pinch vs scroll (the page that
  documents them was not retrievable). They must be measured on the target (see validation plan).
- Related, not usable for us: libinput 1.28 added **3-finger drag** (emulated button held while three fingers move).
  That is the only place libinput itself holds a button during a multi-finger move, and it is a user setting, not
  something we should depend on.

### libgestures (ChromiumOS)

Read: the repository overview, `include/gestures.h`, and doc 73 (which read `immediate_interpreter.cc` etc.).

- Architecture: `HardwareState` frames (touches in mm, buttons, timestamp) go down a stack of interpreters (palm
  classifier, scaling, then `ImmediateInterpreter`), and detected `Gesture` structs go back through a callback.
- Gesture types: `kGestureTypeMove`, `Scroll`, `Swipe`, `SwipeLift`, `FourFingerSwipe`, `FourFingerSwipeLift`,
  `Pinch`, `Fling`, `ButtonsChange`, `Metrics`, `MouseWheel`.
- Continuous data it emits (from the header): `GestureSwipe` and `GestureFourFingerSwipe` = `dx`, `dy` plus
  `ordinal_dx/dy` (unaccelerated), per frame. `GesturePinch` = `dz` (relative zoom factor, 1.0 = no change;
  note the header's own comment has outward < 1.0 and inward > 1.0, the reverse of libinput's absolute scale) plus
  `ordinal_dz` and an explicit `zoom_state` START/UPDATE/END. `SwipeLift`/`FourFingerSwipeLift` mark the release.
- "1:1 workspace dragging": libgestures does **not** implement workspace dragging. It only reports per-frame
  displacement and a lift event; the consumer (Chrome/ChromeOS UI) maps displacement to its own progress and
  decides snap. I did not read the consumer side, so this last point is from the data shape, not from code.
- Value for us: it is the model for the **Android-side** recogniser (already ported in doc 73) and for the *shape of
  the wire data* (begin / delta / lift). It is a library for *detecting* gestures from fingers, not for injecting
  them, so it is not a host-side answer.

### Compositors: GNOME Shell/Mutter and KWin

**GNOME Shell** (read `js/ui/swipeTracker.js`, `main` branch, and `workspacesView.js`):

- It does listen to the libinput gesture, via Mutter: `TouchpadSwipeGesture` handles
  `Clutter.EventType.TOUCHPAD_SWIPE` and reads `event.get_gesture_motion_delta_unaccelerated()`.
- Finger rule: `GESTURE_FINGER_COUNT = 3`, test is `finger_count < GESTURE_FINGER_COUNT` -> propagate, so **3 or more
  fingers (3 and 4) are both accepted**. The tracker itself does not distinguish them (vertical vs horizontal
  orientation and which view is active decide the action).
- Continuous mapping: state `NONE -> PENDING` on the first non-zero delta, accumulate until `DRAG_THRESHOLD_DISTANCE
  = 16`, the dominant axis decides handle/ignore. Then every BEGIN/UPDATE emits `update(time, delta, distance)` and
  `SwipeTracker` adds `delta / distance` to progress with `distance = TOUCHPAD_BASE_WIDTH 400` (horizontal) or
  `TOUCHPAD_BASE_HEIGHT 300` (vertical), in libinput's normalised units. So the view follows the fingers
  continuously, scaled by those constants.
- Release: the last 150 ms (`EVENT_HISTORY_THRESHOLD_MS`) give a velocity; below `VELOCITY_THRESHOLD_TOUCHPAD = 0.6`
  it snaps to the nearest point, above it projects with `DECELERATION_TOUCHPAD = 0.997` and snaps to the projected
  point. **The snap-on-release the user wants is done by the compositor**; we must only send a clean END.
- Honours the system config: it reads `org.gnome.desktop.peripherals.touchpad natural-scroll` and flips the delta.
  This is the "system gesture configuration" that failure 3 ignored.
- Pinch: this tracker handles swipe only. Pinch goes to the focused client through Wayland pointer-gestures.

**KWin** (from web search only, no source read, so lower confidence):

- Touchpad gesture events from libinput are wrapped and fed through an input filter into KWin's gesture/shortcut
  machinery (2017 developer post). Gestures are Wayland-only.
- Defaults are reported inconsistently across versions: desktop switching on 3 or 4 fingers horizontally,
  overview/desktop grid on 4 fingers vertically. Touchpad gestures are largely **not user-configurable**
  (bug 402857 open). Whether the desktop switch animation tracks the fingers 1:1 was **not confirmed**.
- KWin's developer rejected supporting external tools overriding the gestures "for security reasons", so
  libinput-gestures-style daemons do not work there; a virtual touchpad is just another libinput device and is
  not affected.

**Net answer**: yes, both listen to libinput gesture events directly. A host app does **not** talk to the
compositor at all: it creates an input device and the compositor treats it like any other touchpad.

### Touchégg

Read: README/FAQ and the `src/` layout, plus `gesture-controller.cpp` (begin/update/end handlers).

- Gets gestures from libinput ("Under the hood, Touchégg relies on libinput"). Gesture classes: swipe (3+ fingers
  on touchpads, 2+ on touchscreens), pinch, tap (touchscreens only).
- Continuous handling: `GestureController` has `onGestureBegin/Update/End`, logs a `percentage()` on each update and
  forwards to the action, which is how its animations and desktop switching track the fingers.
- Actions: maximise/minimise/tile/close, switch desktop, `SEND_KEYS`, `RUN_COMMAND`, `MOUSE_CLICK`.
- Limits: **"Touchégg only works on X11"** (FAQ); on GNOME it needs a shell extension. It is a *consumer* of
  libinput gestures that performs actions, the opposite direction of what we need, so it cannot forward events to
  our host and is not adaptable as a transport. It confirms the model (begin / update-with-progress / end), and that
  re-implementing the compositor side ourselves would be X11-only work.
- Not verified: how `percentage` is derived (not read).

### Other ways to inject, and why they do not fit

| Path | Gesture support | Verdict |
|---|---|---|
| `uinput` virtual **touchpad** (MT slots + `BTN_TOOL_*`) | libinput derives swipe/pinch/hold itself | **Fits.** Works on any Wayland compositor and no portal prompt. Already how this host injects everything else. |
| `uinput` mouse + keyboard | none | What exists now; discrete, janky. Keep only as fallback. |
| XDG RemoteDesktop portal | `NotifyPointer*`, `NotifyKeyboard*`, `NotifyTouch*`; no gesture method found | Does not help; also adds a consent dialog. |
| `libei` / EIS | Protocol docs on `main` define `ei_gesture_swipe` (`begin(fingers 1..5)`, `update(dx, dy)` logical px, `end(is_cancelled)`) and `ei_gesture_pinch` (`begin(2..5)`, `update(dx, dy, scale absolute, degrees delta)`, `end`), modelled on pointer-gestures. | Interesting, but **I could not confirm** which libei release ships them or that Mutter/KWin implement them (the release notes and merge request were not retrievable). Do not design around it; re-check later. |
| `wlr-virtual-pointer` / virtual-keyboard | none | No gestures; wlroots-only. |
| Touchégg / libinput-gestures | consume gestures | Wrong direction (see above). |

### Phase 2: the architecture challenge, restated

1. A tablet touch surface used as a touchpad is *already* an indirect-pointing device. The data to reproduce is
   "N fingers moved by (dx, dy)" / "2 fingers scaled by s and rotated by r", exactly libinput's gesture payload.
2. The host cannot say "gesture" to the compositor; it can only produce **finger geometry** that libinput will
   recognise. So the Android app must send continuous *gesture state*, and the host must turn it back into
   finger positions on a virtual pad:
   - swipe: place N fingers on a small cluster around the pad centre; each update moves all N by the same
     (dx, dy), converted from mm to device units using the declared axis resolution;
   - pinch: place 2 fingers at +/- d0/2 around the centre; each update sets separation to `d0 * scale` and rotates
     the pair by the angle (libinput reports scale as absolute, so sending absolute scale maps directly);
   - end: lift all fingers (one frame) so libinput emits END and GNOME runs its snap logic. A cancel on Android
     (palm, 5th finger, app pause) must lift fingers in the same way and be remembered so a stuck gesture cannot
     remain open.
3. Pad size matters: synthetic fingers must not run off the pad edge during a long drag (a 1:1 drag across several
   workspaces exceeds any fixed pad). The host should *re-centre* by lifting and re-placing only if needed, or give
   the pad a large coordinate range and accept libinput's own limits. This is the main open design question.

### Phase 3: recommendation



### Best fit

**A uinput virtual multi-touch touchpad driven by continuous gesture state from the tablet**, with the existing
key/wheel path kept only as a fallback. Reasons: it is the only approach where the compositor does the 1:1 drag,
velocity and snap itself (so it feels native by definition), pinch reaches apps as real pinch, the finger-count
policy is the compositor's, it needs no new permission beyond `/dev/uinput`, and it is already the host's mechanism.
`inputtino`'s `trackpad.cpp` is a working blueprint: 16 slots, `ABS_MT_POSITION_X/Y` 0..19200 x 0..10800, tracking
ids, `ABS_MT_PRESSURE` 0..253, `BTN_TOOL_FINGER..QUINTTAP`, `BTN_LEFT`, `INPUT_PROP_POINTER | INPUT_PROP_BUTTONPAD`;
one `SYN_REPORT` per `place_finger`/`release_finger`. Differences to apply: **declare a real resolution** (inputtino
leaves it 0) so libinput's mm-based thresholds and 1000 dpi normalisation behave.

### Data flow

```
Android: MotionEvent -> TouchCapture -> TouchpadEngine (libgestures-style classifier, mm + seconds)
   |  gesture state, per frame:
   |    swipe_begin{fingers}  swipe_update{dx_mm, dy_mm}  swipe_end{cancelled}
   |    pinch_begin{fingers}  pinch_update{dx_mm, dy_mm, scale_abs, rot_deg}  pinch_end{cancelled}
   v
WebSocket (existing input socket, new message types; coalesce to ~one per 8-16 ms)
   v
Rust host: gesture session -> virtual touchpad ("UScreen Gestures", uinput)
   |  finger geometry in device units (mm * resolution), one SYN_REPORT per frame
   v
kernel evdev -> libinput (recognises SWIPE / PINCH) -> Mutter or KWin
   |  GNOME: drags workspace, snaps on release        apps: zwp_pointer_gestures pinch
```

- Android side: the engine already classifies swipe and pinch (doc 73), but only reports *one* discrete
  `swipe(fingers, direction)` and a pinch delta. It needs begin/update/end semantics, and it must **stop deciding
  the finger count**: report the real count and let the host/compositor interpret it. The 3-vs-4 hard-code
  disappears from the app.
- Host side: add a virtual-touchpad constructor beside `new_touch/new_pen/new_pointer` and a small gesture-session
  state machine (Idle / Swipe(n) / Pinch). The old key-combo gesture messages stay for the fallback.
- Fallback selection: use the virtual touchpad when `XDG_SESSION_TYPE=wayland` and the desktop is GNOME or KDE;
  otherwise (X11, Cinnamon, unknown) keep keys, because those stacks never turn libinput gestures into UI actions.
  There is **no generic API to ask the system for its gesture configuration**; we can read what GNOME exposes
  (e.g. natural scroll) only for display, since the compositor already applies it.

### Crates

- **None required.** The host already speaks uinput with `libc` ioctls and constants for `ABS_MT_*`.
- Optional refactor: the `evdev` crate (virtual device builder) or `evdevil` (uinput + multitouch support was seen
  in its docs); only worth it if the raw ioctl code becomes unwieldy. I did not verify versions or API fit.
- Test harness only: the `input` crate (libinput bindings) to assert on received gesture events in an integration
  test. Gesture events exist in its API (seen on docs.rs); not otherwise verified.
- `inputtino` is a C++ library; I found no Rust binding. Use it as a reference, do not depend on it.
- `libei`/`reis` and `ashpd`: only if the libei gesture interfaces become real and widely implemented. Unverified.

### Validation plan (to run before trusting anything; doc 72/73 failed exactly here)

1. With the virtual pad alone: `libinput debug-events` (or `libinput record`) while a script plays a swipe and a
   pinch; confirm BEGIN/UPDATE/END, dx/dy sizes, and scale. This proves libinput accepts our geometry.
2. In GNOME: confirm the workspace follows, and measure the dead zone before BEGIN and the 16-unit GNOME threshold.
3. In a GTK4 or Firefox/Chromium page: confirm native pinch, and what happens with tap-to-click on and off.
4. Only then connect the tablet, and test with a real hand. Log every gesture state transition on both sides.

### Risks

| Risk | Impact | Mitigation / note |
|---|---|---|
| Compositor does not use libinput gestures (X11, Cinnamon/Muffin, XFCE, i3/Sway w/o bindings) | Gesture does nothing | Detect and fall back to the key path; this is the existing behaviour and is documented per DE. |
| libinput re-interprets our geometry (scroll vs pinch; swipe delayed by a threshold; 3-finger drag setting; tap-to-click claims a short 3-finger touch) | Lost first millimetres, wrong gesture | Measure; keep fingers' initial spread/motion symmetric; send an unambiguous start. Cannot be configured from our side. |
| Long drags run off the virtual pad | Gesture stops mid-drag | Large coordinate range, or controlled re-centering; needs a prototype. |
| Compositor thresholds differ (GNOME 16 units, KWin unknown) | Feel differs per DE | Accept; it is native by design. |
| A second, always-present touchpad device changes DE behaviour (disable-while-typing, "touchpad present" UI, tap-to-click, accel profile) | Side effects on the user's real touchpad settings | Create the device only in Touchpad mode (as `ensure_devices_for` does) and destroy on exit. |
| `/dev/uinput` access | Already required today | No new requirement; `groups: input` or ACL. |
| Stuck gesture if the socket drops mid-swipe | Workspace view stuck open | Host timeout lifts all fingers (same safety net as `expire_pinch`). |
| libinput version differences (1.25 here; 1.28 added 3-finger drag) | Behaviour differs per distro | Document tested versions. |
| libei gesture protocol could change or never be implemented | None today | Not used. Re-check before any future migration. |
| KWin behaviour (1:1 tracking, finger counts) only learned from forum/bug text | Plan may not hold on KDE | Test on KDE before claiming support; KDE users are a large part of `docs/compatibility.md`. |
| Real-hardware verification | Docs 72/73 passed synthetic tests and failed on hands | Nothing here is hardware-verified. Treat the plan above as unproven until the validation plan passes. |

### What was and was not read

Read this session: libinput `gestures` page and `event_gesture` API doc; libinput `touchpads` page; libgestures repo
overview and `include/gestures.h`; GNOME Shell `swipeTracker.js` and `workspacesView.js` (main); Touchégg README and
`src/` layout and `gesture-controller.cpp`; inputtino `trackpad.cpp`; libei `ei_gesture_swipe` and `ei_gesture_pinch`
pages; this repo's `uinput.rs`, `events.rs`, `protocol.rs`, `TouchpadEngine.kt`, docs 71/72/73.

Search results only (not opened): KWin gesture behaviour, libinput 3-finger drag, portal RemoteDesktop methods,
Rust crate availability.

Not read / not established: Mutter C source (`meta-seat-impl`, EIS code), KWin source, libinput's swipe/pinch
timeouts, libgestures `immediate_interpreter.cc` (doc 73 covers it), Touchégg's percentage maths, libei release notes
and merge-request status, any real-hardware behaviour.

### Sources

- https://wayland.freedesktop.org/libinput/doc/latest/gestures.html
- https://wayland.freedesktop.org/libinput/doc/latest/api/group__event__gesture.html
- https://wayland.freedesktop.org/libinput/doc/latest/touchpads.html
- https://wayland.freedesktop.org/libinput/doc/1.28.1/drag-3fg.html
- https://chromium.googlesource.com/chromiumos/platform/gestures/
- https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/js/ui/swipeTracker.js
- https://github.com/JoseExposito/touchegg
- https://raw.githubusercontent.com/games-on-whales/inputtino/stable/src/uinput/trackpad.cpp
- https://libinput.pages.freedesktop.org/libei/interfaces/ei_gesture_swipe/index.html
- https://libinput.pages.freedesktop.org/libei/interfaces/ei_gesture_pinch/index.html
- https://blog.martin-graesslin.com/blog/2017/04/how-input-works-touch-screen-edge-swipe-gestures/
- https://bugs.kde.org/show_bug.cgi?id=402857 and https://invent.kde.org/plasma/kwin/-/issues/59


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/75_native_gestures_research.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/75_native_gestures_research.md)
