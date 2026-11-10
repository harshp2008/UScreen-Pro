# 101: Click slop (a tap on selected text no longer starts a drag)

Scope: Android auto mode, **mouse route only** (art route and physical tablets untouched).

## Problem
Tapping blank space or text to unselect, with the usual wobble of a hand when the tip lands, produced a tiny drag in
the app (reported in the Claude desktop app). Reproduced on the previous build with a Chrome probe: a 150 ms tap on
selected text with ~1 mm of wobble fired `dragstart` 6 of 6 times and no click (`cl=0`, selection kept). With ~0.4 mm
of wobble it did not.

## Change
* On a mouse-route DOWN the pointer is frozen at the press point. Samples of that stroke that stay within
  `click_slop_mm` (2.0, `StillCfg`) of the press point write nothing; the first sample outside the zone releases the pointer for the rest of the
  stroke (it follows normally and is not frozen again, so a drag that returns near the start still follows). A lift
  inside the zone releases the button at the press point.
* Distance, not time: the plan said "or ~150 ms", but a time limit would let a pen held still (the long-press menu, a
  deliberate hold) start dragging from jitter after 150 ms, so the zone only ends by distance.
* A repeated DOWN and a hover inside a stroke go through the same rule. Not applied on the art route (every sample
  reaches the tablet and the pointer), eraser strokes, or physical tablets.
* `scripts/tablet_inject.py`: new `pen-tap X Y MS AMP` (a tap with a wobble of AMP pixels).

## Results (GNOME 46 Wayland, SM-X616B over USB adb, Chrome probe, 6 taps per row)
| Wobble during a 150 ms tap on selected text | Before | After |
|---|---|---|
| ~1.0 mm (10 px) | `dragstart` 6/6, click lost | `dragstart` 0/6, `cl=1`, selection cleared |
| ~0.4 mm (4 px) | no drag, click ok | no drag, `cl=1`, selection cleared |
| ~3.1 mm (30 px) | drag | drag 6/6 (outside the zone, as intended) |

Regression: Chrome drag-select 12/12 (`BTN_LEFT` 12/12); 3 quick taps `pd=3`; art-route stroke tip 1/1, pressure 0.23;
focus flip mid-drag `sel=12`; resting pen on the mouse route silent for 5.5 s. Host tests: 169 passed.

## Not verified
* The Claude desktop app itself (the app where it was reported); Chrome stands in for it.
* The 2 mm default is a guess from the injected wobble; it is not yet a setting (planned with the other settings, P8).
* The pointer jumps up to the zone radius when it leaves the zone (no easing).
