---
title: "96 Fix unified ghost tracking (clicks dropped, invisible physical cursor, stuck button)"
sidebar:
  order: 55
  label: "96 Fix unified ghost tracking (clicks dropped, invisible phy"
---

Follow-up to doc 95. Method: reproduce with real clients first (Chrome via DevTools titles, a fullscreen GTK3 probe) driven by the
real daemon, the tablet's virtual HID pen (`scripts/tablet_inject.py`) and a throwaway uinput pen, then fix, rebuild, re-measure.

| Field | Value |
| :-- | :-- |
| **ID** | 96 |
| **Date** | not recorded |
| **Build** | `f3866a3-dirty` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Root causes (measured, not assumed)

| Symptom | Cause | Evidence |
|---|---|---|
| Android clicks/taps dropped | Not the extension or focus: with unknown focus the host already sent `BTN_LEFT` (a GTK client got press/motion/release). The pen device re-entered **proximity in the same millisecond as every button press** (and left 40 ms after), which makes clients treat it as a pen session. | libinput: `POINTER_BUTTON` and `TABLET_TOOL_PROXIMITY proximity-in` share a timestamp. Chrome: 3 quick taps -> only 1 `pointerdown` (Wayland and XWayland). |
| Physical cursor invisible | Physical samples went through `apply_pen`, which only ghost-tracks when the *Android pen's* `stylus_style` is `auto`. With no Android profile the slot uses the global default, still `mouse` in this config, in `display` mode -> plain tablet path -> no pointer events. | `config.toml`: global `stylus_style = "mouse"`, `input_mode = "display"`. Also: a still pen produced unchanged `ABS_X/Y`, which the kernel drops, so the cursor could hide after 300 ms of tablet-only events. |
| Stuck button after a drag | Android drops `ACTION_CANCEL` in Stylus mode (`if (stylusMode) return true`), and `setInputBlocked` lifts fingers but not the pen, so a cancelled stroke never sends its pen-up and `BTN_LEFT` stays held. | `TouchCapture.kt` (before) |


## Result

### Changes

* **Mouse route keeps the tablet tool out of range** (`ghost.rs`). Pointer follows every sample and owns the tip (`BTN_LEFT`);
  the tablet device is only in range on the pen route (art app by focus, or forced with Ctrl+Alt+P / `uscreen toggle-tablet`).
  Unknown focus -> mouse route, so the pen is a plain mouse out of the box without the extension. This narrows doc 95's "both
  devices always": measured, keeping the tool in range is what broke taps.
* **Physical tablet always ghost-tracks** (`apply_physical`), whatever `stylus_style` is (not on X11, not without the pointer).
  Debug logs added: `Physical tablet moved to X,Y`, `Injected absolute mouse to X,Y` (`RUST_LOG=uscreen=debug`).
* **Still-pen nudge**: when the tool is in range and the position repeats, the pointer is written 1/65535 aside and back, so a
  noisy but stationary pen keeps producing pointer events.
* **Panic release** (`ghost_panic_release`): on pen-up, range exit, Android cancel, disconnect and mode change, `BTN_LEFT` plus
  the tablet's `BTN_TOUCH`/`BTN_TOOL_PEN`/`BTN_TOOL_RUBBER`/pressure are forced up on both devices; cancel/disconnect/mode change
  also force `BTN_RIGHT`, `BTN_MIDDLE` and `BTN_STYLUS`. Deviation from the brief: up/exit do **not** force right/middle/barrel,
  because Android sends a range exit with the barrel button deliberately held just before the tip lands (see `sendPenProximityExit`);
  the barrel keeps its own press/release lifecycle. The kernel drops a release for a key that is not down, so the extra releases
  are free.
* **Android**: `ACTION_CANCEL` (and input being blocked mid-stroke) now sends the pen-up (`liftPen`).
* `UScreen Pointer` already has `INPUT_PROP_POINTER` and udev classifies it `ID_INPUT_MOUSE=1` (checked, unchanged).

### Results (this machine: GNOME 46 Wayland, SM-X616B, new APK `f3866a3-dirty`)

| Test | Result |
|---|---|
| Chrome (Wayland), Android pen: hover | `types=mouse` only: the tablet tool never enters range |
| Chrome, drag | `pd=1 pu=1 md=1 mu=1 cl=1`, text selected |
| Chrome, 3 quick taps | `pd` 1 -> 4: **3 of 3** (was 1 of 3) |
| Stuck test: pen vanishes mid-drag | libinput `BTN_LEFT` pressed=5 released=5; Chrome `md=mu=5` |
| Physical (throwaway pen `dead:beef`), profile style **mouse** (old code: tablet only) | 61 pointer motions, 2 clicks; Chrome `pd=2`, `sel=42`, press x=281 / release x=1125 of 1536 (expected 281 / 1125); tablet device 0 events |
| Pen route (`toggle-tablet`), Android pen | libinput: tablet tip down=4 up=4, `POINTER_BUTTON`=0; route flips back (`now mouse`) |
| Host tests | 125 passed (new: tap sequence, tool-out-of-range, route flip, nudge, panic release, lift without down) |


## Not verified

* Pen-route delivery into a client: Chrome (Wayland) ignores tablet tips (`pd=0` in pen route); a GTK3 probe also gets no press
  from the tablet device even with the original plain `tablet` style (baseline), so neither can confirm Krita. **Krita drawing with
  pressure is untested**; it needs the extension (re-login) or `uscreen toggle-tablet` while Krita is focused.
* The real XP-Pen (no pen contact was possible); the real S Pen (the virtual HID pen stands in).
* Seeing the cursor (I cannot see the screen) and a physical mouse taking over after a drag: shown only as button state (press
  count = release count), not by moving a real mouse.

## Sources

- [`experiments_and_findings/96_fix_ghost_tracking_unified.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/96_fix_ghost_tracking_unified.md)
