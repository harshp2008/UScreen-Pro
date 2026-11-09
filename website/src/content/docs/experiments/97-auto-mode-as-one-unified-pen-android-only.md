---
title: "97 Auto mode as one unified pen (Android only)"
sidebar:
  order: 56
  label: "97 Auto mode as one unified pen (Android only)"
---

Scope: Android `stylus_style = "auto"` only. Physical tablets are not touched (`apply_physical` still uses `ghost_pen`).
`mouse` and `tablet` styles are not touched (same branches of `apply_pen` as before).

| Field | Value |
| :-- | :-- |
| **ID** | 97 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### What was wrong

Auto picked a *route* per sample: unknown or non-art focus kept the tablet tool out of range, so the `UScreen Pen`
device got nothing (no pressure, no tilt). Krita then had no pressure whenever the focus was unknown.

### Change (`host/src/input/ghost.rs::auto_pen`, called from `events.rs::apply_pen`)

| Part | Behaviour in auto |
|---|---|
| `UScreen Pen` | Always: tool in range, X/Y, pressure, tilt, whatever the focus (or none) |
| `UScreen Pointer` | Always: absolute X/Y, so the cursor is always visible |
| Tip (the only switch) | Art app: `BTN_TOUCH` on `UScreen Pen`. Unknown or other app: `BTN_LEFT` on `UScreen Pointer`. Tablet data keeps flowing either way |
| Pen up | `BTN_LEFT`, tablet contact and `BTN_TOOL_PEN` released on both devices |
| Range exit (Android sends it just before every touch-down) | `BTN_LEFT` and the tablet contact released at once; the tool leaves range after 150 ms unless the pen is back (`auto_flush_exit`, 50 ms tick). This keeps the tool from leaving and re-entering range around a click |
| Cancel / disconnect / mode change | `release_all` (pointer buttons incl. right/middle, tablet contact, tool, barrel) |

`ghost_pen` is unchanged and still serves the physical tablet.


## Result

### Deviations from the brief

* Pen-up and range exit do not force `BTN_RIGHT` up. Android keeps the barrel button level across strokes and only
  sends a release when the pen stays away (`releasePenButtons`); forcing it up would break barrel+tip strokes. Cancel,
  disconnect and mode change do release everything. Android reports a cancel to the host as a pen-up (`liftPen`).
* libinput derives the tablet tip from pressure, so on the non-art route the tablet also reports tip down/up (4 down /
  4 up for 4 strokes) next to the `BTN_LEFT` click. That is the cost of never dropping pressure.

### Results (GNOME 46 Wayland, SM-X616B over adb TCP, virtual HID pen on the tablet, libinput + Chrome probe)

| Test | Result |
|---|---|
| Auto, unknown focus, Chrome (Wayland) | drag `pd=1 pu=1 md=1 mu=1 cl=1`, `sel=12` (text selected); 3 quick taps: 4 of 4 clicks total (3 of 3 taps) |
| Auto, same run, libinput | tablet: 100 axis events (pressure up to 0.23, tilt -0.35/-0.35), 20 proximity, 8 tip; pointer: 124 absolute motions, `BTN_LEFT` 4 pressed / 4 released |
| Auto, unknown focus, Chrome (XWayland) | 4 of 4 clicks |
| Auto, forced pen route (`uscreen toggle-tablet`) | tablet tips 4 down / 4 up, `BTN_LEFT` 0, pointer still moves (126 motions) |
| Mouse style | pointer only: 108 motions, `BTN_LEFT` 4/4; tablet device 0 events; Chrome `types=mouse` |
| Graphic tablet style | tablet only: 103 axis, 8 tips; pointer 5 motions (the park-on-exit that already existed), `BTN_LEFT` 0 |
| Stuck: injector killed mid-drag (auto) | `BTN_LEFT` 1 pressed / 1 released; tablet tip 1 down / 1 up; proximity 2 in / 2 out |
| Host tests | 133 passed (8 new auto tests) |


## Not verified

* **Krita is not installed**: no pressure/tilt confirmed inside a drawing app. Verified only at libinput (what Mutter
  receives). Whether Krita draws once or twice when it gets both a `BTN_LEFT` press and a tablet contact (unknown
  focus) is untested.
* Chrome ignores tablet-device tips on Wayland and XWayland (`pd=0` in the pen route and in graphic tablet style, as before).
* The GNOME extension is not loaded in this session (needs a re-login), so art-app routing by focus was exercised only
  with `toggle-tablet`.
* The injected pen's tilt is a constant -0.35; large tilts were not exercised. Seeing the cursor on screen and a real mouse
  taking over after a drag were not checked by eye.

## Sources

- [`experiments_and_findings/97_auto_mode_unified_tablet.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/97_auto_mode_unified_tablet.md)
