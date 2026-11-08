# 76: Native virtual touchpad (raw fingers to libinput)

Branch `feat/upstream-sync`, nothing committed. Implements the approach recommended in
`75_native_gestures_research.md`. Tablet: Galaxy Tab S9 FE+ (SM-X616B). Host: Ubuntu, GNOME on Wayland, libinput 1.25.0.

## What changed

| Area | Change |
|---|---|
| Host device | New uinput device **`UScreen Touchpad`** (`UInputDevice::new_touchpad`, `host/src/input/uinput.rs`): MT protocol B, 10 slots, `ABS_MT_SLOT/TRACKING_ID/POSITION_X/POSITION_Y/PRESSURE` (plus `ABS_X/Y/PRESSURE` mirror), `BTN_LEFT`, `BTN_TOUCH`, `BTN_TOOL_FINGER..QUINTTAP`, `INPUT_PROP_POINTER` + `INPUT_PROP_BUTTONPAD`. Size 400 x 260 mm at 40 units/mm (a real resolution: libinput measures every threshold in mm). |
| Lifetime | Created only while `input_mode == touchpad` and native gestures are in effect (`ensure_devices_for(mode, native)`); removed (fingers lifted first) in Stylus / Display mode or when the setting goes off. |
| Wire | New message `{"type":"touchpad_raw","slots":[{"id":0,"x":120.5,"y":35.0,"pressure":0.4}]}`. `slots` is **every finger down right now**; `x`/`y` are **millimetres** from the corner of the touch area (not pixels: the host pad has a physical size). An empty list lifts everything. |
| Host injection | `inject_pad` diffs the set against the pad: lifted ids get `ABS_MT_TRACKING_ID -1`, new ids the lowest free slot and a fresh tracking id, the finger-count `BTN_TOOL_*` and `BTN_TOUCH` follow the count, one `SYN_REPORT` per frame. State-based on purpose: a dropped message can never leave a finger stuck. `release_all` / mode change / disconnect lift the pad. |
| Android | `TouchCapture.feedRaw`: in Touchpad mode with native gestures it sends the raw fingers and runs **no** gesture engine. Shape restriction (`outsideIds`, clamping to the rectangle) and palm / pen filters are applied to the raw coordinates first. |
| Bottom-right click | A finger that lands **alone** in the click strip is held back (like a finger in a clickpad's button area). A short, still tap (<= 300 ms, <= 3 mm) there sends a right click through the host's relative mouse. If a second finger lands, the held finger joins the gesture instead. |
| Naming | The relative mouse used to be called `UScreen Touchpad`; it is now **`UScreen Mouse`** (product `0x0006`) and exists in touchpad mode for the right-click strip and the wake nudge (and for everything when native gestures are off). |
| Setting | `native_gestures` (global default + per-device override), default **on**: `config.rs`, `options.rs`, tablet settings sheet, Tauri GUI "Use Native Gestures (Wayland/libinput)" in the Touchpad section. |
| Fallback | `DeviceOptions::native_gestures_effective()` = setting on **and** Wayland **and** GNOME or KDE. The effective value is what `config_update` sends to the tablet, so on X11, Cinnamon, wlroots compositors the tablet goes back to its own gesture engine and the host back to key combos (`Super+PageUp/PageDown`, `Super`, Ctrl + wheel). Those key paths are untouched; they simply no longer run when native gestures are in effect. |

Not done as written: the brief said to remove all key-combo gesture code and also to keep it as the fallback.
The fallback won; the key code stays and is only reachable when native gestures are not in effect.

## Verification

### libinput sees a real clickpad (this machine, real uinput device, no tablet)

`cargo test -p uscreen real_pad -- --ignored --nocapture` replays scripted fingers on the real device while
`libinput debug-events` listens:

```
DEVICE_ADDED  UScreen Touchpad  cap:pg  size 400x260mm tap(dl off) left scroll-nat scroll-2fg-edge click-buttonareas-clickfinger dwt-on dwtp-on
POINTER_MOTION         (+40.00/+10.00 unaccelerated per frame)           <- 1 finger
POINTER_SCROLL_FINGER  vert 10.52                                        <- 2 fingers
GESTURE_SWIPE_BEGIN 3 / GESTURE_SWIPE_UPDATE 3 -19.2/0.0 (-15.77/0.00 unaccelerated) x ~60 / GESTURE_SWIPE_END   <- 3 fingers
GESTURE_SWIPE_BEGIN 4 ...                                                                                          <- 4 fingers
GESTURE_PINCH_BEGIN 2 / GESTURE_PINCH_UPDATE  1.24, 1.82, 2.40, 2.98 ... / GESTURE_PINCH_END                        <- pinch out
```

Capability `pg` = pointer + gesture. Updates are continuous per frame and the pinch scale is libinput's absolute
scale. `tap(dl off)`: libinput's own default for the device is tap off, GNOME applies the user's setting (see below).

### End to end, with the tablet (single finger only)

`./build-all.sh` passed (APK, daemon, GUI) and deployed; the daemon restarted and installed the APK
(`33282d3-dirty -> 243a3a9-dirty`), the tablet reconnected, and the daemon log shows
`uinput touchpad 'UScreen Touchpad' created`. With the tablet in Touchpad mode,
`adb shell input swipe` on the tablet produced 120 `POINTER_MOTION` events from the `UScreen Touchpad` device in
`libinput debug-events` (tablet -> WebSocket `touchpad_raw` -> host -> uinput -> libinput). `adb shell input tap` in the
bottom-right strip produced `BTN_RIGHT` pressed / released on `UScreen Mouse`. `adb` cannot inject more than one
finger, so **3 / 4 finger swipes and pinch from the tablet itself were not exercised**; they were exercised only by
the scripted replay above.

A plain `adb input tap` outside the strip produced no click in `libinput debug-events`. That tool shows libinput's
default (tap off); GNOME applies its own setting (`tap-to-click` is `true` here), and a 1 ms synthetic tap is not a
real finger, so this is inconclusive, not a pass.

### Unit tests

Host: new tests for the message, slot / tracking-id / lift (-1) sequence, slot reuse and clamping, finger-count
tool buttons, pad removal on leaving Touchpad mode, and the Wayland + GNOME/KDE rule. Android: `HostLinkTest`
for the setting. `cargo test -p uscreen`: 101 passed, 0 failed, 2 ignored (the real-device player and one older test). Android `testDebugUnitTest`: BUILD SUCCESSFUL.

## What I could not verify

- **Nothing was run with a real hand on the tablet**, and I did not watch GNOME's workspace follow it. docs 72/73
  passed synthetic tests and failed on real hardware; the same warning applies here. The libinput check above proves
  the device and the geometry; it does not prove the feel.
- Whether Android's `getPressure()` mapped into 50..90 of 255 is always accepted as a touching finger by libinput on
  this tablet (it is for the synthetic values).
- Taps: libinput's tap-to-click follows the user's GNOME setting for touchpads (on, on this machine). If it is off,
  the tablet can click only with the bottom-right strip (right click); left click needs tap-to-click.
- Two-finger / three-finger tap right and middle click are now libinput's (tap-to-click map), not the tablet's.
- KDE: expected to behave the same (KWin reads libinput gestures) but untested.
- The 1:1 workspace drag with 3 fingers depends on GNOME's own thresholds (16 units before it commits).

## GUI toggle

Touchpad section of the Tauri GUI: **Use Native Gestures (Wayland/libinput)**, default on, per-device override like
the other touchpad settings. Seen in the Vite dev build of the same UI (mocked backend, `devMock.ts`), not in the
installed Tauri window: the toggle renders, is on by default and the console is clean.
