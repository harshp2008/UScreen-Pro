---
title: "73 Input engine rewrite v2, based on proven libraries"
sidebar:
  order: 48
  label: "73 Input engine rewrite v2, based on proven libraries"
---

Branch `feat/upstream-sync`. Supersedes `72_input_engine_rewrite.md`.

| Field | Value |
| :-- | :-- |
| **ID** | 73 |
| **Date** | not recorded |
| **Build** | `33282d3` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Why v2

The v1 engine (doc 72) passed its own synthetic tests and failed on a real hand and a real S Pen. The
tests proved the code did what the code said; they did not prove that the code matched how fingers and
pens behave. v2 starts from implementations that already work on real hardware, ports their rules (and
their numbers) instead of inventing ones, and tests with input that has the roughness of a real hand
(contact size, pressure, jitter, finger-by-finger arrival).

Phase 0 (revert, commit `33282d3`) removed the v1 code. This document is Phase 1 (research, no
implementation code written during it) and the architecture that came out of it.

### Sources, and what was actually read

| Project | Read? | Notes |
|---|---|---|
| libchrome-gestures (`chromiumos/platform/gestures`, BSD-3) | **Yes, source** | `immediate_interpreter.cc`, `palm_classifying_filter_interpreter.cc`, `tap_to_click_manager.cc`. The reference. |
| Winlator / Axon / InkBridge | Yes, source | Read in v1; used here only for Android-side details (below). |
| S-Pen-Mouse (`sr-tream`) | **Yes, source** | `MouseButtons.java`. |
| inputtino (`games-on-whales`) | **Yes, source** | `pentablet.cpp`, `trackpad.cpp`, `mouse.cpp`. |
| Rust `evdev` crate | Partly | Docs only; the project already uses it, and its behaviour (emit appends `SYN_REPORT`) was learned in v1. |
| PhonePad | **No** | The repository and `docs/ARCHITECTURE.md` return 404 for the owner name given. Only the bullet points of the project page were available (sub-pixel remainder, two-finger right click, drag lock). The behaviours are implemented, but they are not ported from its code. |
| compose-stylus (`MohamedRejeb`) | **No** | Repository 404. The `PenButton None/Primary/Secondary/Tertiary` model is taken from the task description, not from code. |

### libchrome-gestures: what it does and what is taken

libgestures is the code behind the ChromeOS touchpad. Input is a `HardwareState` (a timestamp plus a list
of `FingerState`: tracking id, position, touch major/minor, pressure, flags). A chain of filters feeds an
`ImmediateInterpreter`, which turns the frames into move / scroll / pinch / swipe / button gestures.
Everything is in **millimetres and seconds**, which is why its constants carry over between devices.
Our tablet reports pixels, so v2 converts with the screen's `xdpi`/`ydpi` before anything else runs.

### Our problems, and how libgestures solves each

| Problem | libgestures answer | Taken as |
|---|---|---|
| Taps missed or turned into drags | `TapToClickManager` state machine (Idle, FirstTapBegan, TapComplete, SubsequentTapBegan, Drag, DragRelease, DragRetouch); `Tap Timeout` 0.2 s; `Tap Move Distance` 2 mm; tap-drag timeout 0.3 s; inter-tap 0.15 s | `TapToClick.kt`, same states and numbers. The tap is judged on the *set* of fingers (`TapRecord`: touched / released sets, `TapComplete` = all touched are released), not on the first lift. |
| Finger count wrong for 2/3 finger taps | `NoteTouch` only accepts a new finger if it is *close enough* to an existing one (`CloseEnoughToGesture`); `TapType`: 1 = left, >1 = right, exactly 3 = middle | Same rule, same mapping. Fingers far apart (a resting thumb, the other hand) do not join the tap. |
| Palm resting on the glass | `PalmClassifyingFilterInterpreter`: palm if pressure >= 200, width >= 21.2 mm, several fingers >= 75 mm; fat-finger ratios 1.4 (pressure) / 1.3 (width); edge zone 14 mm, tap exclusion border 8 mm; stationary palm: 2 s and < 4 mm; eval timeout 0.1 s | `PalmFilter.kt`. Size and pressure come from `getTouchMajor/Minor/getPressure`, not only `TOOL_TYPE_PALM` (which Samsung does not always set). |
| Scroll vs pinch | `GetTwoFingerGestureType` + `UpdatePinchState`: displacement vectors of both fingers; dot product < 0 means moving apart/together (pinch guess), > 0 means same direction (scroll). `ScrollAngle` rejects scroll when the normalised dot < -0.2. Pinch is *locked* once certain (both fingers moved 8 mm, or after 0.1 s of consistent guess) and stays until the fingers lift. | `TouchpadEngine.kt` uses the same decision with the same numbers, including the lock. |
| Jumpy pointer when a finger is added or lifted | `Change Timeout` 0.2 s and `Change Min Move Distance` 3 mm: after the finger count changes, no motion until the fingers have moved 3 mm again | Same: the reference position is reset on every count change, and motion is held until 3 mm of travel. |
| 3 and 4 finger swipes | `GetMultiFingerGestureType`: sort the fingers along the dominant axis; all deltas from the start positions must have the **same sign** (otherwise the start positions are reset); the largest delta must be >= 1.5 mm and the smallest >= 20% of the largest | Same rule. The v1 engine required a long travel and a tight vertical angle, which a real hand does not give. |
| Slow-motion stalls | `Move Lock Speed` / `Move Report Distance` 0.35 mm: small motions are accumulated, not dropped | Accumulate in mm, emit when over the report distance; remainder kept. |
| Buttons from the pad area | `GetButtonTypeFromPosition`: bottom-right zone gives right button | Kept as the bottom-right 15% zone (already a setting). |

Not taken: thumb heuristics tied to a physical click, `Fling`, and acceleration curves (the existing
`gain` setting stays). Not enough information about a physical button on a touchscreen to make them useful.

### PhonePad (bullet points only)

Sub-pixel remainder on the relative motion, two-finger tap as right click, drag lock. The host already
keeps a sub-pixel remainder (`rel_rest` in `events.rs`); the tablet also keeps the fraction between frames
so the two layers cannot disagree. Drag lock is libgestures' `Tap Drag Lock Enable` (off by default there,
off here).

### compose-stylus / S-Pen-Mouse: pen buttons

S-Pen-Mouse (`MouseButtons.java`) does not react to button *events*; it keeps the **previous level** of
(tip, barrel button) and sends the **difference** on every frame. That is robust against missed
`ACTION_BUTTON_PRESS/RELEASE` events, which Samsung does not always deliver. Taken: the pen state is
recomputed from `MotionEvent.buttonState` on every `DOWN`, `MOVE`, `UP` and hover frame and diffed
against what the host was last told. Mapping: tip = left (held while the tip is down, which is what makes
drag-select work), `BUTTON_STYLUS_PRIMARY` = right (configurable to middle), `BUTTON_STYLUS_SECONDARY` =
middle (configurable). The `PenButton` None/Primary/Secondary/Tertiary names are used for the setting
values.

### inputtino: virtual devices

- `pentablet.cpp`: pen tablet gets `ABS_X/Y/PRESSURE/TILT_X/TILT_Y`, `BTN_TOOL_PEN/RUBBER`, `BTN_TOUCH`,
  `BTN_STYLUS/STYLUS2`; property `INPUT_PROP_POINTER` (and `DIRECT` for a screen tablet).
- `trackpad.cpp`: multitouch slots 0..15, `BTN_TOOL_FINGER/DOUBLETAP/TRIPLETAP/QUADTAP/QUINTTAP`, properties
  `INPUT_PROP_POINTER` + `INPUT_PROP_BUTTONPAD`. A real trackpad *device* would let libinput do the whole
  gesture recognition on the host; that was not chosen because the recognition has to stay on the tablet
  (the user's tablet screen is the touch surface and shapes/zones are tablet-side).
- `mouse.cpp`: relative mouse = `REL_X/Y`, wheel + hi-res wheel, `BTN_LEFT..TASK`, `INPUT_PROP_POINTER`,
  `MSC_SCAN` before each button. Same layout as our `UScreen Pointer`.

### Where the brief and the evidence disagree: "send REL alongside ABS"

The brief asks to send `REL_X/REL_Y` alongside `ABS_X/ABS_Y` on the stylus device so the cursor is
visible. inputtino's pen tablet and its mouse are two separate devices; neither sends REL and ABS on one
device. A device that emits both moves the cursor twice in libinput (once as an absolute pointer, once as
a relative one), so v2 does **not** do that. The stylus-as-mouse path moves the cursor with the existing
absolute pointer and emits `BTN_LEFT` on the pointer device, which is a real pointer (`INPUT_PROP_POINTER`,
no `DIRECT`, no `TOUCH`), so the compositor draws its cursor. The graphics-tablet path (`stylus_style =
tablet`) keeps the pen device untouched.

### Android-side details (from v1 research)

- Winlator `TouchpadView`: one tap record per pointer id, a tap is <= 200 ms and < 10 units; the click is
  press, 30 ms gap, release (a zero-length click is dropped by some hosts).
- Axon `TouchpadScreen`: coalesce moves to ~16 ms; tap then touch again to drag within 300 ms.
- InkBridge `TouchListener`: stylus-only mode drops `TOOL_TYPE_FINGER`; reads `historySize` samples.
- Pointer **ids**, not indices, identify fingers (indices are repacked on every lift).

### Architecture (as built)

```
MotionEvent --> TouchCapture --> HardwareState (mm, s) --> TouchpadEngine
                                                            |- PalmFilter          (palm_classifying_filter)
                                                            |- TapToClick          (tap_to_click_manager)
                                                            |- GestureClassifier   (immediate_interpreter)
                                                            v
                                              Output: move / button / scroll / gesture
                                                            |
                                                       WebSocket (rel, button, scroll, gesture)
                                                            |
                                                  host: UScreen Pointer / Keyboard (uinput)
```

- `TouchpadEngine` has no Android types; frames in, actions out, so it is unit-tested with recorded
  real-hand-like frames.
- Stylus: strict isolation (`TOOL_TYPE_FINGER` ignored in stylus mode, only `STYLUS`/`ERASER`);
  `stylus_style = mouse` (default) sends hover and drag as an absolute pointer with `BTN_LEFT` held from
  `DOWN` to `UP`; `tablet` keeps the pressure/tilt pen device.
- Diagnostics: `adb logcat -s UScreenInput` logs every frame summary and every decision (palm, tap state,
  gesture type) so a failure on real hardware can be read afterwards instead of guessed.

### Honest limits

- Nothing here has been run with real fingers or a real S Pen by me. The synthetic tests now carry contact
  size, pressure, per-finger timing and jitter, but that is still a model.
- Palm thresholds are libgestures' numbers on its devices; Samsung's `touchMajor` is in pixels of an
  unknown calibration. If real palms are missed or real fingers rejected, the diagnostic log shows the
  values to tune.
- PhonePad and compose-stylus were not readable (see table).


## Result

not recorded as a separate section; see What was tested.

## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/73_input_engine_rewrite_v2.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/73_input_engine_rewrite_v2.md)
