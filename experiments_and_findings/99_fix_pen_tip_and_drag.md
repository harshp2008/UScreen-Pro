# 99: Fix unresponsive pen tip and dropped drag-select (follow-up to 98)

Method: reproduce first (libinput on the real virtual devices, a Chrome probe page with a repeatable drag, the tablet's
virtual HID pen), then change, rebuild, re-measure. The findings below were measured; where they differ from the brief's
guess, the measurement wins.

## What was actually wrong

| # | Finding | Evidence |
|---|---|---|
| 1 | **The sequence filter dropped real pen events** (the brief's Task 3 symptom, but not from reordering). The tablet's `rendered` frame acks carry the video frame counter in a field also called `seq`; the filter took it for an input number. In Extended Display mode (video running) the counter runs ahead of the input counter, so DOWN/MOVE/UP with a lower number were discarded | Reproduced on the 98 build in display mode (video running): 13 `Discarded a stale or duplicate input message` lines in one 8-drag run. In that run libinput still saw all 8 presses, so what was dropped there were moves/hovers; whether a DOWN is lost depends on how far the frame counter is ahead, which I did not force. My 98 matrix ran in stylus mode (no video), so it never saw it |
| 2 | **Mouse route kept the tablet tool in range and fed it squeezed pressure** during a click/drag (98's "tip suppression"). Clients bound to both devices (Krita, Chrome's tablet path) see a pen session next to a mouse button. The brief's Task 2 (`just send the movement to UScreen Pointer`) and doc 96's measurement agree: the tablet device must be silent on the mouse route | doc 96: tool in range at the press dropped taps (1 of 3). 98/97 reintroduced it |
| 3 | **A focus change or hover exit in mid-stroke released the button** (98's route-change `force_release_all`, a range exit that lasts, and `emit`'s inserted release before a second `BTN_LEFT` down). Any of these cut a drag; a focus source that reports an empty/transient focus would cut every Krita stroke | read from `ghost.rs`/`uinput.rs` at 98; now covered by tests |
| 4 | **Krita never gets the art route on this machine.** The GNOME extension *loaded and enabled* (`uscreen-app-lock@uscreen.app`, version 2) is the old copy: it has no pen routing, so `~/.local/share/uscreen/focus.json` is never written, focus is always unknown and the route is always the mouse. `scripts/install.sh` copies the new extension; it was not re-run since pen routing was added | installed `extension.js` has 0 occurrences of `focus.json` (repo copy has it); `GetWindows` over D-Bus is `AccessDenied` for other clients, so the host cannot ask for the focus itself |

## Changes

* **`protocol.rs` sequence filter**: only numbered *input* messages are filtered (`classify` reads `type` and `action`; `rendered` and everything else
  pass). MOVE and hover are never dropped. A DOWN/UP/button is dropped only if its exact `seq` was already accepted (duplicate); a DOWN is
  also dropped only if a *later-numbered UP* was already accepted (stale half of a finished stroke), and an UP only if a later-numbered DOWN was.
  A DOWN that arrives after later MOVEs is **accepted**, so the drag is not lost.
* **`ghost.rs::auto_pen`** rewritten (physical tablets still use `ghost_pen`, untouched):
  * **Mouse route**: pointer gets X/Y (written first), then `BTN_LEFT` down; moves go to the pointer only; the tablet device gets no event at all
    (tool never in range). UP releases.
  * **Art route**: tablet gets the tool, X/Y, tilt, `BTN_TOUCH` and the real, unmodified `ABS_PRESSURE` (eraser end -> `BTN_TOOL_RUBBER` contact); no `BTN_LEFT`;
    the pointer still follows without a button.
  * **Route latched at DOWN**: a focus change in mid-stroke does not move, cut or release the stroke. `force_release_all` runs on UP (cancel arrives as an UP),
    disconnect, a mode/style change by the user, and a range exit that lasts 150 ms *with no stroke open*.
  * A DOWN while the stroke is open and fresh (< 500 ms since the last sample) is the drag continuing: no release, no press. A MOVE with no stroke open (its DOWN was
    lost) starts the stroke (`WARN` logged), unless it is within 100 ms of a lift (a straggler). A range exit inside a stroke is ignored.
* **`uinput.rs`**: removed the "left DOWN inserts a release" guard from `emit` (it broke a hold on any repeated DOWN). Held-key tracking (`DevState`, `force_release_all`) stays.
* Android app unchanged (its `seq` stays; the host just stops misreading the frame acks).

## Results (GNOME 46 Wayland, SM-X616B over adb TCP, build `267e318-dirty`; focus simulated with `focus.json`)

| Test | Result |
|---|---|
| Host unit tests | 154 passed |
| Chrome drag-select, auto, stylus mode, 12 drags (6 hover-settled, 6 not) | 12/12: `pd=1 md=1 cl=1 dbl=0 sel=12` each; tablet device 0 events |
| 12 drags, libinput vs host, stylus and display mode | host DOWN=12/13, libinput `BTN_LEFT` pressed 12 / released 12 in both modes; `Discarded` = 0 (display mode was 13 on the 98 build) |
| Art route (Krita focus), stroke | tablet: tip down 1 / up 1, pressure up to 0.23 (the injector's pen pressure, unscaled), proximity 4/4; pointer: 0 buttons, 70 motions |
| Chrome, 3 quick taps | `pd=3 pu=3 md=3 mu=3 cl=3` (UI clicks work) |
| Focus flips Chrome -> Krita in the middle of a 240-step drag | `pd=1 pu=1 sel=12`; pointer `BTN_LEFT` 1 press / 1 release per stroke; tablet 0 tips; the release is the last button event of the stroke |

libinput on `UScreen Pointer`, one Chrome drag (no `BTN_LEFT` event between press and release):

```
 event19  POINTER_MOTION_ABSOLUTE +1.681s	 27.34/ 43.75
 event19  POINTER_BUTTON          +1.772s	BTN_LEFT (272) pressed, seat count: 1
 event19  POINTER_MOTION_ABSOLUTE +1.813s	 28.05/ 43.75
 event19  POINTER_MOTION_ABSOLUTE +1.848s	 28.75/ 43.75
 event19  POINTER_MOTION_ABSOLUTE +1.870s	 29.49/ 43.75
 event19  POINTER_MOTION_ABSOLUTE +1.892s	 30.19/ 43.75
 event19  POINTER_MOTION_ABSOLUTE +1.916s	 30.90/ 43.75
   ... 55 more POINTER_MOTION_ABSOLUTE events ...
 event19  POINTER_MOTION_ABSOLUTE +3.199s	 68.87/ 43.75
 event19  POINTER_MOTION_ABSOLUTE +3.221s	 69.57/ 43.75
 event19  POINTER_MOTION_ABSOLUTE +3.242s	 70.31/ 43.75
 event19  POINTER_BUTTON          +3.267s	BTN_LEFT (272) released, seat count: 0
```

## Not verified / caveats

* **Krita itself was not driven.** It is installed (flatpak), but automating its GUI means injecting keystrokes and screenshots on the live desktop, where the
  Claude window had focus; I did not do that. Art route and pressure are proven at libinput (what Mutter hands out), not inside Krita.
* **Krita gets the mouse route until the new GNOME extension is loaded.** Install it (`scripts/install.sh`, or copy `extensions/uscreen-app-lock/{metadata.json,extension.js}`
  and `schemas/*.gschema.xml` into `~/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/`) and **log out and in** (Wayland loads extension code only at login).
  Until then `uscreen toggle-tablet` flips the route by hand. Krita on the mouse route should now draw as a plain mouse stroke (pointer only, no tablet device in range),
  without pressure. I could not install it from this session (the copy was blocked).
* The real S Pen was not used; the tablet's virtual HID pen stands in (as in 96-98). The display-mode Chrome numbers are not meaningful (the pen maps to the virtual display,
  not Chrome's window); display mode was judged by libinput counts and the discard log.
* Supersedes parts of 98: the "pressure squeezed under the tip threshold", the "force release on route change" and the "left DOWN is always an edge" rules, and the
  98 matrix's test 3 (a stroke now stays on the device it started on instead of ending at the focus change).
