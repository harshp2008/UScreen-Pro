# 72: Input engine rewrite

The previous touchpad and stylus work (docs 69 and 71) did not hold up in real use. My earlier verification
fed synthetic events through the plumbing and I reported it as working; real hands and a real pen showed it was
not. This document records what was removed, what was studied, what replaced it, and what is still unproven.

## Phase 0: what was reverted

Nothing of that work had been committed (the tree held 75 changed paths, much of it earlier uncommitted work), so
`git revert` was impossible. I first committed everything as a snapshot (`b0eaebb`, local only) so the revert is a
real diff, then removed the broken parts by hand (`8e3d3fb`, message as requested).

Removed:
- The stylus profile system: `host/src/stylus_profile.rs`, the registry and its watcher, the `stylus-*` CLI commands,
  `stylus_event` / profile messages, the vendored S Pen Remote SDK (`android/spen-remote/`) and `SamsungSPen.kt`,
  the profile manager in the tablet app and in the GUI (`StylusProfileManager.tsx`, Tauri `stylus.rs`), the
  `stylus_profile` setting everywhere.
- The virtual gesture pad (`UScreen Gestures`) and the `transform` messages, and my rewritten gesture engine
  (`TouchpadGestures.kt` restored to the upstream version from `d146e16`, then replaced in Phase 2).
- The four `scripts/e2e_*.sh` scripts built around them. **Kept:** `scripts/tablet_inject.py`, the generic tool that
  makes real multi-touch and a real pen on the tablet over `/dev/uhid`; it is how everything below was checked.
- The S Pen SDK entry in `THIRD_PARTY_LICENSES.md` and the changelog entries for removed features. Docs 69 and 71
  carry a "superseded" note.

Kept: the settings plumbing (global default, per-device override), the touchpad shape filter, the Stylus-mode device
policy (no relative mouse), haptics, the wizard warnings.

**Config.** Global `input_mode` set to `display`; the per-device `input_mode` override removed from the tablet's
profile (it had been `touchpad`; before my first test runs it was `stylus`); the removed attempt's `stylus_profile`
key deleted. I did not delete the tablet's profile wholesale: it also holds its Wi-Fi address and the settings you
chose (immersive mode, touch input, ...), and removing those would unpair it. `touchpad_shape = "wide_rectangle"`
on that profile is yours and was left.

Side effect of the snapshot commit: it also recorded some `android/spen-remote/build/` outputs that were not
git-ignored; the revert commit deletes them, so only the history contains them.

## Phase 1: reference implementations

| Project | Found | What was taken |
|---|---|---|
| Winlator `TouchpadView` | The URL in the brief is 404. The file is at `moze30/winlator-glibc/.../glibc/widget/TouchpadView.java` (736 lines, read in full). | Fingers tracked per pointer id (max 4) with start position and time; tap = < 200 ms and barely moved; **multi-finger tap judged when the first finger lifts, all fingers must still be taps**; pointer boost above a speed threshold (6, x1.25); click press and release 30 ms apart; stylus: hover moves the pointer, button state selects right click. Not taken: rounding without a remainder (the thing PhonePad fixes), absolute "simulated touchscreen" mode. |
| Axon | Exists (Go server + Kotlin/Compose client). README names the gestures; `TouchpadScreen.kt` read. | Move deltas coalesced and flushed about every 16 ms; double tap within 300 ms then drag holds the left button; scroll vs pinch decided by which moved more, with a 1.1 bias to scroll. Not taken: Ctrl +/- key presses for zoom. |
| InkBridge | Exists. `TouchListener.kt` read. | `stylusOnly` drops `TOOL_TYPE_FINGER` outright; historical samples are sent too; `requestDisallowInterceptTouchEvent(true)`; tilt radians to degrees. The brief says it does palm rejection; the code has none beyond the finger/stylus split. |
| PhonePad | The page exists but only lists "sub-pixel move remainder accumulation" without describing it; I could not find its repository, so nothing was read. | Technique implemented from the name alone: carry the fraction of each move over to the next. |
| S-Pen-Mouse | Exists. README only (it uses Shizuku and reads the pen's evdev device; not `MotionEvent`). | The mapping idea: hover moves the pointer, touch is left, the pen button is a held right button. |

## Phase 2 to 4: the new engine

**Touchpad** (`TouchpadEngine.kt`, pure maths in `TouchpadMath.kt`):
- A `Finger` per pointer id (max 5) with start position/time. Fingers outside the shape, palm-flagged fingers and a
  sixth finger are ignored for their whole life; a finger sliding out stops at the edge.
- One finger: pointer movement with a 2 mm dead zone (so taps never nudge it), then everything since touch-down,
  scaled by 1.6, boosted above 6 px/event, with the fractional part carried over (`SubPixel`).
- Android's `GestureDetector` does tap (left, or right in the bottom-right 15% of the shape) and double-tap-and-drag
  (left held). Its slop is scaled by giving it a context at 1.6x density, because its phone-tuned defaults are
  too tight for a fingertip on a tablet.
- Two fingers: scroll or pinch, decided once per gesture (`TwoFingerClassifier`, movement is released at the
  decision so nothing is lost). The pinch distance comes from Android's `ScaleGestureDetector`.
- Several fingers (judged at the first lift, like Winlator): 3 = middle click, 4 = workspace prev, 5 = workspace
  next, 2 = right click only with the "two-finger tap" method. Tolerances are millimetres, from the panel density.
- Clicks are press, 30 ms, release.

**Pinch (a real bug found while testing in Chrome).** The host pressed Ctrl, turned the wheel and released Ctrl for
every single step. Ctrl comes from the keyboard device and the wheel from the mouse device and the compositor does
not order events across devices, so most wheel events reached the browser without Ctrl and scrolled instead of
zooming: a Chrome probe page received 0 to 3 pinch events for two pinches. The host now holds Ctrl for the whole
pinch (`pinch` / `pinch_end`, with a 12 ms lead before the first wheel step, a watchdog that lets go after 700 ms of
silence and release on disconnect). The same probe now receives 44 (22 zoom-in, 22 zoom-out).

**Stylus** (`TouchCapture.kt`, `events.rs`): fingers and anything not stylus/eraser are ignored in Stylus mode (on the
tablet, and again on the host); the side button is pen action 5/6 and the second button 7/8; each is a setting
(`stylus_button_primary` default middle click, `stylus_button_secondary` default right click; also left click, the
tablet stylus button, or nothing), in the tablet app and the GUI. A mouse action is held for as long as the pen
button is (drag works) and released as whatever the press took, even if the setting changed meanwhile. In Stylus mode
the plain pointer also follows the hovering pen, so an ordinary mouse cursor is always under it.

**Host**: `UScreen Touchpad` is created with `INPUT_PROP_POINTER` only (read back from sysfs: properties = 1; no
direct, buttonpad or semi-mt bits). Devices a mode needs are created when it is entered, not at the first event.

**Config drift fixed.** `daemon.rs` wrote every change of slot 0's mode (including a per-device override) back as the
global `input_mode`, so switching one tablet to Touchpad made `display` the default for all. Only the tray's
"Graphics tablet" switch asks for a global change now (`FileConfig::set_global_stylus`, tested).

## Verification

`cargo test`: 100 pass. `cargo clippy --workspace --all-targets -- -D warnings`: clean. Android unit tests pass
(touchpad maths, settings). `./build-all.sh` exits 0; the APK is installed on the Tab S9 FE+.

Real multi-touch and a real pen were created on the tablet with `scripts/tablet_inject.py` and the host's virtual
devices read back with `scripts/e2e_input_engine.sh` (BROWSER=1 adds Chrome). Standard Rectangle, Touchpad mode:

- Taps outside the shape: nothing. Inside: `BTN_LEFT`. Bottom-right inside: `BTN_RIGHT`; bottom-left: `BTN_LEFT`;
  bottom-right outside the shape: nothing.
- 3 fingers drifting 18 px, landing 15 ms apart, 100 ms tap: `BTN_MIDDLE`. 4 fingers: Meta+PageUp. 2 fingers (zone
  method): nothing.
- Slow drag, 40 px in 100 steps: 72 units in 23 events (the remainder is carried, the pointer does not stall or
  stutter). Fast drag, 400 px in 25 steps: 800 units (1.6 x 1.25 boost). Two-finger scroll 300 px: 7 wheel notches.
  Tap then touch and drag: click, then left held while moving, then released.
- Pinch in Chrome (Wayland kiosk): 44 ctrl+wheel events, 22 zoom-in then 22 zoom-out.
- `UScreen Touchpad` properties bitmask 1 (`INPUT_PROP_POINTER` only).

Stylus mode (a per-device override): finger tap, finger drag and 3-finger tap: nothing on any device. Pen hover:
position reports on the pen device and the same number on the pointer. Side button held 400 ms: `BTN_MIDDLE` down
then up on the pointer; after changing the setting to right click, `BTN_RIGHT`. Devices present: Touch, Pen,
Pointer, Keyboard (no Touchpad).

Config: the global `input_mode` stayed `display` while the tablet's own override went touchpad, then stylus.

## Not proven

- Real fingers and a real S Pen. The HID injection is the closest I can get; my previous round passed the same
  kind of test and still failed for you, so treat this one as necessary, not sufficient. Specifically unproven:
  that a real hand's three-finger tap stays within 4 mm and 350 ms, that Samsung's panel does not flag one of
  three flat fingers as a palm (a palm-flagged finger is deliberately ignored and would turn a three-finger tap
  into a two-finger one), and that the pen's real hover and button behave like the injected one.
- "The mouse is invisible": I could not observe the cursor and did not reproduce it. Two things changed that bear
  on it (fingers no longer reach the touch device in Stylus mode, which hides the pointer in GNOME; the pointer
  now follows the pen's hover), neither confirmed on screen.
- Pinch zoom amount is tuned by a constant (6 wheel units per pixel of finger travel); the feel is a guess.
- PhonePad's sub-pixel handling was implemented from its description, not from its code.
- The tablet app's new pen-button rows were not opened on the tablet; the setting travelled the config path in
  the host checks above.
