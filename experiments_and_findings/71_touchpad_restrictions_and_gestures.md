# 71: Touchpad shape, native gestures, multi-finger taps, S Pen debugging

Branch `feat/upstream-sync`, nothing committed. Tablet: Galaxy Tab S9 FE+ (SM-X616B, 2560x1600).

## What changed

| Task | Result |
|---|---|
| 1. Touchpad Shape | New setting `touchpad_shape` (`full_screen` default, `standard_rectangle` = centred 4:3, `wide_rectangle` = centred 16:9) in `config.rs`, `options.rs`, `config_update`, `HostConfig.kt`, tablet settings and the GUI (Touchpad section, per-device override). `TouchpadShape.bounds()` computes the rect. A finger that lands outside it is ignored for its whole life (nothing is sent, no gesture sees it); one that slides out stops at the edge. The tablet draws the rect (lighter panel, faint border) on pure black. |
| 2. Click zone | Now relative to the rect: the bottom 15% of the rect, right half, right-clicks on a single-finger tap; a faint line and divider are drawn inside the rect. |
| 3. Native pinch / rotate | The tablet sends the zoom factor and rotation in degrees per step (`transform`, `transform_end`; the same two numbers `detectTransformGestures` reports). The host replays them as two fingers on a virtual multi-touch touchpad (`UScreen Gestures`), so libinput emits real `GESTURE_PINCH_*` events (scale and angle) and the application handles them natively. If that device cannot be created it falls back to Ctrl + wheel. No app detection is needed. |
| 4. 3 / 4 finger taps | Fixed (see root cause below). 3 = middle click, 4 = Meta+PageUp, 5 = Meta+PageDown. |
| 5. S Pen profile | See findings. Stylus mode now has no relative mouse and no gesture pad; profile clicks and scrolls go through the absolute pointer. In Touchpad mode no profile can fire. The S Pen Remote SDK never runs in Touchpad mode. |

Parity: `touchpad_shape` is a normal global-default / per-device-override setting, shown in the tablet's Display & Input section and in the GUI's Touchpad section.

## Deviations from the brief

- **No Compose `pointerInput` / `detectTransformGestures` / `awaitEachGesture`.** All finger input already arrives through one `SurfaceView` touch listener shared by Display, Touchpad and Stylus modes, and the gesture engine is a pure class with unit tests. Moving only Touchpad to Compose would split the pipeline. The engine computes the same quantities (zoom factor, rotation) itself, and the rect filter lives in `TouchCapture`.
- **Standard Rectangle is 4:3, Wide is 16:9.** On a 16:10 panel a 16:10 rect would be the whole screen. Wide (16:9) is 2560x1440 on this tablet, so it only trims 10% of the height.
- Four fingers go to Meta+PageUp as asked; five fingers do Meta+PageDown.

## Root causes found

- **3/4 finger taps never fired**: the movement tolerance was a fixed 12 px, which is 1.2 mm on this panel; fingers landing together shift more than that, so the tap was classified as a scroll. With three or more fingers down the old code also scrolled on any 12 px drift, and the whole tap had to finish in 250 ms. Now: tolerance is physical (1.5 mm single finger, 4 mm with three or more fingers down, set from the panel's dpi), 3+ fingers never scroll or transform, and a multi-finger tap may take 400 ms.
- **First click after entering Touchpad mode could be lost**: the relative mouse is made lazily and the compositor needs a moment to see a new device. With the new Stylus-mode policy (the mouse is destroyed there) that became visible on every switch back, so the devices a mode needs are now created as soon as the mode is entered (`ensure_devices_for`).
- **Injection harness bug** (not product): `cat > /dev/uhid` merges back-to-back writes, which dropped the lifts at the end of a gesture; the injector now paces writes.

## S Pen profile investigation (Task 5)

1. On this tablet the SDK answers `UNSUPPORTED_DEVICE` (logcat: `SPen: Unsupported device`). The Tab S9 FE+'s S Pen has no Bluetooth / Air Actions, so the SDK never binds and the Samsung profile cannot affect the Android side at all. The app retried the connection on every settings update; it now remembers the answer for the process and the settings page says why the air gestures do nothing.
2. A virtual pen digitizer on the tablet (real `MotionEvent`s) hovered across the screen with each profile, four runs alternating: Standard 366 / 366 and Samsung 373 / 370 position reports at the host's `UScreen Pen`, same ~22 ms median gap and same max gap (the pause between moves). **No difference between the profiles in the pen stream**; the host's profile code returns "no mapping" for the side button under both, so `BTN_STYLUS` passes through identically.
3. The profile never touches finger events (they are not routed through it) and does not stop the standard stylus pipeline.
4. I could not reproduce "the mouse sometimes does not move or show". Likely contributor, fixed earlier: before the Stylus isolation fix fingers were sent to `UScreen Touch`, and touch input hides the pointer in GNOME. What is still possible and untested: the cursor-hiding behaviour of a tablet tool leaving proximity (the absolute pointer parks the cursor on exit).
5. Device policy verified: in Touchpad mode `UScreen Touchpad`, `UScreen Gestures` and `UScreen Keyboard` exist; after switching to Stylus the Touchpad and Gestures devices are gone and only Touch, Pen, Pointer and Keyboard remain.

## Verification

`cargo test`: 105 pass (new: gesture pad frames, rotation direction, pinch restart, Stylus-mode device policy, profile clicks through the pointer, eager devices). clippy clean. Android unit tests (new: rect maths, tolerant 3-finger tap, transform, zone relative to rect). `./build-all.sh` exit 0, APK installed.

Real multi-touch is injected into the tablet through a virtual HID touchscreen and pen created over `/dev/uhid` (`scripts/tablet_inject.py`; the kernel input nodes are not writable from adb), then the host's virtual devices are read back (`scripts/e2e_touchpad_native.sh`, Standard Rectangle, Samsung profile active, Touchpad mode):

- taps outside the rect (x=100 and x=2500): nothing. Tap inside: `BTN_LEFT`.
- tap at the bottom-right inside the rect: `BTN_RIGHT`; bottom-left: `BTN_LEFT`; bottom-right outside the rect: nothing.
- 3 fingers, drifting 18 px, landing 15 ms apart: `BTN_MIDDLE`. 4 fingers: `KEY_LEFTMETA` + `KEY_PAGEUP`. 2 fingers (zone method): nothing.
- pinch apart / together and a 70-degree rotate: libinput `GESTURE_PINCH_BEGIN/UPDATE/END` from `UScreen Gestures` (about 40 updates each; the pad restarts the gesture when the fingers run out of room, hence two begins for the long pinch).
- Chrome (Wayland kiosk, `scripts/e2e_browser_pinch.sh`): the page received 103 ctrl+wheel pinch events, 40 zoom-in for the pinch apart and 43 zoom-out for pinch together.
- Screenshots: `screenshots/71_gui_touchpad_shape.jpg`, `71_tablet_touchpad_standard_rectangle.png`.

## Not verified / known

- Real fingers and a real S Pen were not used; the HID injection is the closest available. Rotation gave deltaY about 0 in Chrome (a browser has no use for it); apps such as GNOME Maps or an image viewer would show it, none was tried.
- The first tablet-side Touchpad Shape selection in the app's settings sheet was not tapped through on the tablet; the setting travels the same path as the others and the rect was seen drawn via the config.
- Chrome ran in kiosk mode on the desktop for about 20 s during the browser test.
- **Pre-existing daemon behaviour, not changed**: `daemon.rs` persists a change of slot 0's mode (including a per-device override) as the *global* `input_mode` (stylus vs display). Switching one tablet to Touchpad therefore rewrites the global default to `display`. It made my test runs drift the config; the scripts now restore and verify it.
