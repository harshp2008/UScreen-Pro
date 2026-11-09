# 100: Resting pen no longer takes the cursor (hover park filter) + pen recorder

Scope: Android auto mode (`auto_pen`). Physical tablets (`ghost_pen`) and the mouse/tablet styles are untouched.
First part (P1 + P2) of the plan: pen stillness engine, hover park/wake filter, flight recorder.

## Problem
A pen lying on the tablet is still in range; the tablet keeps sending hover samples with a little noise. Each one was
written to `UScreen Pointer` (plus a one-unit nudge for a still pen, and tablet frames on the art route), so the pen
kept taking the cursor from a mouse/touchpad ("sometimes it works, sometimes it freezes"). Not reproduced on demand.

## Change
* `stillness.rs` (new, pure, unit-tested): `HoverFilter`. Distances in mm from the normalised pen position and the
  tablet's physical size (`tablet_mm`, set from the app's resolution message, default 266x166).
  Defaults: park after 300 ms inside 1.5 mm; wake beyond 5 mm (larger, so noise cannot wake it). Touch, lift and range
  exit reset it. Verdicts: Follow / Park / Skip / Wake.
* `ghost.rs::auto_pen`, hover (action 3) with no stroke open: Follow/Wake write as before; **Park** takes the tablet tool
  out of range (art route) and writes nothing; **Skip** writes nothing at all (no pointer position, no nudge, no tablet frame).
  A hover inside a stroke is unchanged. Touching down from a parked pen works (the filter resets on DOWN).
* `recorder.rs` (new): in-memory ring of 4000 pen samples with what the router did (`hover`, `park`, `skip`, `wake`,
  `down`, `up`, `move`, `exit`, `button`); hover skips sampled 1/25, moves 1/10. `uscreen debug-dump` writes it to
  `~/.local/share/uscreen/pen_dump.txt` via the running daemon (request file, same mechanism as `toggle-tablet`).
* `scripts/tablet_inject.py`: new `pen-rest X Y SECONDS` (pen in range, 1 px of noise, 100 Hz).

## Results (GNOME 46 Wayland, SM-X616B, build `267e318-dirty`; libinput on the real virtual devices)
| Test | Result |
|---|---|
| Host unit tests | 165 passed (filter, recorder, auto-mode park/wake/touch-from-parked) |
| Pen rests 5 s, mouse route | pointer silent for the whole rest (longest gap 5.5 s), 57 motions in the whole run (approach, first 300 ms, wake move) |
| Pen rests 5 s, art route | pointer silent 5.5 s; tablet axis silent 5.5 s; tool left range (prox 3 in / 3 out), came back on the wake move |
| Chrome drag-select, 12 drags | 12/12, `BTN_LEFT` 12/12, tablet 0 events |
| Art route stroke / 3 taps / focus flip mid-drag | unchanged: tip 1/1, pressure 0.23; `pd=3`; `sel=12`, one press/release |
| `uscreen debug-dump` | wrote 710 samples; `park` and `wake` rows present |

## Not verified
* The real S Pen's resting noise is unknown. The injected rest is 1 px (~0.1 mm). If a real resting pen wobbles more than 1.5 mm
  it will not park; the dump (`uscreen debug-dump`, run right after the mouse freezes) will show its actual noise and the
  radii can be tuned (they are `StillCfg`, not yet a setting).
* A pen lying with the *tip touching* (a held press) is not handled by this filter; the dump would show it as `down` without `up`.
* Android does not send hover distance, so it is not used.
* The "yield to a real mouse" backup is not built (add only if the dump shows the filter is not enough).
