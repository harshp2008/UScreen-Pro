# 88: Touch jump, laggy pointer and finicky clicks on the native virtual touchpad

Follows `76_native_virtual_touchpad.md`. Input: the `libinput debug-events` log from a real hand on the tablet.

## What the log actually shows

| Observation | Reading |
|---|---|
| `kernel bug: Touch jump detected and discarded` (5 times in the first 90 ms, then rate limited) | libinput saw a single-frame jump on a finger. |
| Almost every motion pair looks like `-0.58 (-3)` then `-6.64 (-23)`, 2 ms apart, every ~16 ms | The host replays what it receives, and libinput on the host handles evenly spaced **and** 2 ms paired frames correctly (replayed with `PAD_SCRIPT=glide_even,glide_pairs`: a constant 32 units in every frame, both ways). So the unevenness is in what the tablet sends. |
| `GESTURE_HOLD_BEGIN` followed by `GESTURE_HOLD_END cancelled` | Normal libinput behaviour: hold begins when a finger rests, and is cancelled as soon as it moves. It is not a "dropped click" and a coordinate jump does not cause it. Real touchpads produce the same pair. |
| 3-finger swipes arrive as `SWIPE_BEGIN 4`, several end as `cancelled` | The finger count libinput reports is the count it sees in `BTN_TOOL_*`; a count that flips during a gesture cancels it. Android reported a 4th contact (palm / resting thumb) or a 3-finger set that briefly became 4. Not fixed here, see "Not changed". |
| `UScreen Mouse 2` sends +1/-1 pixel nudges at every touch | The wake nudge: the tablet sends `wake` on the first finger down and the host moves the relative mouse out and back so an auto-hidden cursor reappears. It fires in the middle of the pad's own motion. |
| Two sets of devices (`UScreen Touch`..`Touchpad` and `... 2`) | Instance 0 and instance 1. This machine has the same tablet registered twice (USB and the Wi-Fi adb address); the events go to instance 1. Not a code fault. |

## Root cause of the lag: Android batches and resamples touches

By default Android hands a view its touches once per display frame and **resamples** the newest point to the frame
time. The newest point is an interpolated or extrapolated position slightly ahead of the real samples, while the
batched history is the real samples. Sent in order, the stream becomes: history sample (behind the last "current"),
then the new current (ahead): one tiny step and one large step 2 ms apart, every frame. libinput's pointer
acceleration and its swipe / jump detection both reason about the speed of each step, so a stream like this gives a
jerky, lagging pointer, no consistent acceleration, and now and then a "jump".

Fix: `view.requestUnbufferedDispatch(event)` on the first finger down in Touchpad mode. With it Android delivers
every touch sample as the panel reports it, no batching to the frame and no resampling, which is the stream a
real touchpad produces.

## Changes

| Where | Change |
|---|---|
| Android `TouchCapture.setSurfaceView` | `requestUnbufferedDispatch` on `ACTION_DOWN` in Touchpad mode. |
| Android `TouchCapture.feedRaw` | No `wake` on finger down in native mode. Coordinates are now the touch area **centred** on the host's 640 x 420 mm pad and scaled by the sensitivity. |
| Host `Wake` handler | No 1 px nudge in native mode (a second device twitching next to the pad). Activity still counts for the idle dim through `touchpad_raw`. |
| Host pad | 400 x 260 mm to **640 x 420 mm**. The tablet sits in the middle, so with sensitivity up to 200 % even a 300 mm wide tablet stays clear of libinput's edge-palm zones and the clickpad's bottom button area, where libinput ignores motion. |
| **Sensitivity** | New setting `touchpad_sensitivity` (percent, 50-200, default 100) on the tablet (Settings, Touchpad) and in the Tauri GUI (Touchpad section), with a per-device override. It scales the finger's physical travel before libinput sees it, so it changes pointer and swipe travel together. Pointer acceleration (the adaptive profile) is libinput's and its speed is your desktop's touchpad speed setting. |
| Tests | Host: clamp expectation for the new pad size; config_update key list. Android: `touchpadSensitivityDefaultsTo100AndStaysInRange`. `PAD_SCRIPT` gained `glide_even` and `glide_pairs`. |

## Not changed, on purpose

* **Taps stay libinput's.** The brief asked Android to send `TRACKING_ID`, `POSITION`, `SYN`, `BTN_LEFT 1`, `SYN`,
  `BTN_LEFT 0`, `SYN`, `TRACKING_ID -1`. A `BTN_LEFT` from the pad is a *physical click* of a clickpad. libinput
  already turns a short, still touch into a left click when tap-to-click is on (GNOME: on). Sending a button as well
  would give two clicks per tap. The sequence the host writes already has the tracking id and position established
  before anything else (`inject_pad`), and the finger-count tool buttons follow the finger set.
* **The old relative-mouse engine** remains the fallback when native gestures are off or unsupported.
* **Finger count flips (3 vs 4)** depend on what Android reports; palm filtering is the existing `isPalm` /
  `isPenLike` logic.

## Verification

See the bottom of this file (filled in after the build and deploy).

### Done on this machine

* `./build-all.sh`: APK, daemon and GUI built and deployed; `uscreen.service` restarted; the tablet's app was
  updated (`243a3a9-dirty -> 7cd5648-dirty`). `cargo test -p uscreen`: 101 passed, 0 failed, 2 ignored. Android
  `testDebugUnitTest`: passed (including `touchpadSensitivityDefaultsTo100AndStaysInRange`).
* Host replay through the real device with `libinput debug-events --enable-tap` on the new 640 x 420 mm pad:
  tap -> `BTN_LEFT` pressed / released (libinput's own tap), 3-finger swipe -> `GESTURE_SWIPE_BEGIN/END 3`, pinch ->
  `GESTURE_PINCH_BEGIN/END 2`, 0 "Touch jump" warnings.
* Tablet to host to libinput with `adb shell input swipe` (one synthetic finger) after the deploy: 129
  `POINTER_MOTION` events, all from `UScreen Touchpad`, none from `UScreen Mouse` (no wake nudge), steady ~8 ms
  cadence with near-equal deltas, 0 "Touch jump" warnings.

### Not verified (needs your hand)

* The unbuffered dispatch is the fix for the alternating tiny / large steps, derived from the log and from the
  replay that rules out the host. `adb input` events are not batched or resampled the way real touches are, so
  the effect on real fingers has to be seen in a new `libinput debug-events` from the tablet: the motion
  deltas should now be of similar size from frame to frame, and the jump warning gone.
* 3-finger swipes registering as 4 (a resting thumb or palm counted by Android) are not addressed.
* The sensitivity slider and its GUI field were built but not clicked through.
