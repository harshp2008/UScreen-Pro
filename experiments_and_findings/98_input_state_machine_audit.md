# 98: Input state machine audit (Android auto mode)

Method: read the code at `267e318` (HEAD before this change), prove or disprove each claim with line references, then
fix, rebuild and test live (libinput on the real virtual devices, Chrome probe, the tablet's virtual HID pen, and a
scripted WebSocket client). Line numbers below are HEAD's.

## Phase 1: audit

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| 1 | Krita double-draw | **Confirmed at the compositor, client behaviour unverified** | `ghost.rs:417-440`: on the mouse route the tip goes to the pointer (`BTN_LEFT`) *and* the tablet gets `pressure_contact` (`m.pressure = pressure_contact`, :427, :440). libinput derives a tablet tip from pressure alone (measured with a probe device: tip down at 230 of 4096, none at 200), so one stroke produced `POINTER_BUTTON` and `TABLET_TOOL_TIP` (doc 97: 4 clicks and 4 tablet tips for 4 strokes). A client bound to both devices gets two strokes. Krita is not installed here, so only the compositor side is proven |
| 2 | Route-change state leak (`on_focus_changed`) | **Partly confirmed; the named function does not exist** | There is no focus callback: `set_focus` (`ghost.rs:136`) only stores the app, and the route is recomputed per pen sample. `tip` is fixed at DOWN, so the UP releases the right device (no permanent stick), but (a) nothing ends the stroke when the focus changes: the old app keeps receiving the stroke until the pen lifts, (b) with the pen held still no sample arrives, so nothing is noticed at all, (c) a hotkey flip does the same. Net: a stroke leaks into the wrong app for its whole length |
| 3 | Stuck pen tip / `BTN_TOOL_PEN` | **Confirmed in a narrower form** | In the plain tablet path `inject_pen` UP releases `tool` chosen from *that sample's* `eraser` flag (`uinput.rs:780`, :828). Android's cancel lift sends `eraser=false` (`TouchCapture.kt` `liftPen`, :728). A stroke started with the eraser end and lifted by a cancel leaves `BTN_TOOL_RUBBER` down. Also `release_all` released the pen only `if self.pen_proximity` (`uinput.rs:1009`), a flag kept apart from what the device was actually told, so a desync skipped the release. DOWN/UP pairing itself is fine in auto mode (`tip` pairs them; `ghost_panic_release` clears both tool keys) |
| 4 | Wi-Fi duplicate / out-of-order | **Confirmed: no validation anywhere; plus a cross-connection race** | `InputEvent::Pen` has no sequence or timestamp (`protocol.rs`), and `server.rs:468` passes every parsed message straight to `handle_event`. One TCP/WebSocket connection cannot itself duplicate or reorder, so on a single link the exposure is the app sending a DOWN twice or two event sources. The real network race is the **reconnect**: every connection is its own task (`server.rs:243`), an old connection still draining after the tablet reconnected keeps feeding events, and when it ends it calls `release_all()` (`server.rs:496`), cutting a stroke the new connection already started |
| 5 | Ghost cursor stuck on touchpad hand-over | **Confirmed for style and display<->touchpad changes** | The release on a mode change hangs off `pen_only_rx` (`server.rs:134`, :169), which only flips when `input_mode` crosses stylus/non-stylus. `stylus_style` auto<->mouse<->tablet, and display<->touchpad, never trigger it, so a held barrel/right button or `BTN_LEFT` survives a switch. Stylus->touchpad does release. The held-button record is also parallel bookkeeping (`pen_mouse_buttons`, `mouse_buttons`, `tip`, `pen_proximity`) that can disagree with what a device holds |

Additional finding: the 150 ms range-exit grace added in doc 97 released only `BTN_LEFT` and the contact on exit and let
the tool go later, but `BTN_RIGHT` (barrel) was never released by a range exit that lasts; it relied on Android's own
barrel release.

## Phase 2: what changed

* **Tracked state per device** (`uinput.rs`): every `UInputDevice` records the keys it was told are down (`held`, updated in
  the single `emit`), so `DevState` (`Idle` / `Hovering` / `Down(mask)`) is derived from what the device really holds,
  whatever code path emitted. `force_release_all(Which::{Pen, Pointer, Touchpad, All})` releases from that record, zeroes
  pressure and resets the router's bookkeeping.
* **`BTN_LEFT` DOWN is always an edge**: `emit` inserts a release if left is still held, so a duplicated or reordered DOWN
  cannot be swallowed; auto mode also clears a held tip contact/left before every DOWN (`auto_idle_before_down`).
* **Force release on**: route change (on a sample, and on a 50 ms clock for a still pen: `auto_note_route`,
  `auto_check_route`), `stylus_style` or `input_mode` change (options watcher in `server.rs`), a range exit that lasts
  150 ms (`auto_flush_exit`: both devices, incl. the right button), disconnect and mode change (`release_all` ends with
  `force_release_all(All)`). Cancel reaches the host as a pen-up (`liftPen`) and ends in the same release.
  UP stays the light release (left, contact, tool) so a held barrel keeps its own lifecycle.
* **Route change mid-stroke**: the stroke on the old device ends cleanly and does not restart on the new one; the pen
  hovers until its next DOWN.
* **Double-draw suppressed**: on the mouse route the tablet gets the tool in range, X/Y, tilt and pressure **squeezed under
  libinput's tip threshold** (`TIP_PRESSURE_CAP = 150` of 4096, `below_tip`), never `BTN_TOUCH`. This is the one
  deviation from "send pressure": libinput ties tip to pressure, so any pressure above ~5 % *is* a stroke. Real
  pressure goes only to the art route.
* **Sequence numbers**: the Android app numbers every input message (`seq`, one lock for number + send). The host's
  per-connection `SeqFilter` discards a message whose number is not higher than the last (duplicate or straggler);
  messages without `seq` (older app) pass. Duplicate DOWNs without a number are also ignored when their stroke is open
  and under 40 ms old.
* **Newest connection wins**: a superseded connection stops feeding events and no longer releases the devices when it
  closes.
* **Tool keys**: an UP releases whichever tool key is held (`inject_pen`), not just the one the sample names.

Interpretation notes: "Idle before BTN_LEFT DOWN" is applied to the left button and the tip contact; a held barrel/right
button is deliberately not cleared by a DOWN (barrel+tip is a right-drag). `UScreen Touchpad` is the relative mouse plus the
multi-touch pad (`Which::Touchpad`).

## Phase 3: test matrix

Live on this machine (GNOME 46 Wayland, SM-X616B over adb TCP). The pen is the tablet's virtual HID pen
(`scripts/tablet_inject.py`); focus is simulated by writing `focus.json` (Krita = art app, Chrome = not); what the
compositor receives is read with `libinput debug-events` on `UScreen Pen` and `UScreen Pointer`; Chrome is a probe page.
Tests 9 and 10 use a scripted WebSocket client (authenticated with the session token) because one TCP connection cannot
duplicate or reorder by itself.

| # | Test | Expected | Result | Evidence (libinput) |
|---|---|---|---|---|
| 1 | Auto, Krita focused, draw line | single stroke with pressure | **PASS** | tablet: 1 tip down / 1 up, pressure up to 0.23; pointer: `BTN_LEFT` 0 |
| 2 | Auto, Chrome focused, drag-select | text selected, no double-click | **PASS** | Chrome `pd=1 md=1 cl=1 dbl=0 sel=12`; pointer `BTN_LEFT` 1/1; tablet tips 0, max pressure 0.00 (under the tip threshold) |
| 3 | Auto, draw in Krita, switch to Chrome mid-stroke | stroke ends cleanly, Chrome does not draw | **PASS** | tablet tip up at event 71 of 236 (the switch, not the stroke's end); pointer `BTN_LEFT` 0; 233 pointer motions continued |
| 4 | Auto, hover in Krita, click in Chrome | Chrome gets the click, Krita no stroke | **PASS** | pointer `BTN_LEFT` 1/1; tablet tips 0 |
| 5 | Auto, hold pen button, switch to touchpad mode | pen button released, touchpad works | **PASS** | `BTN_RIGHT` 1/1 and `BTN_LEFT` 1/1, with 131 pointer motions *after* the right release (released at the switch, mid-drag); `UScreen Touchpad` then produced 107 motions from a finger drag |
| 6 | Auto, pen disconnected mid-drag (app killed) | left and right released | **PASS** | `BTN_LEFT` 1/1, `BTN_RIGHT` 2/2; tablet proximity 4 in / 4 out |
| 7 | Mouse mode, draw | mouse only, no tablet events | **PASS** | tablet: 0 events of any kind; pointer `BTN_LEFT` 1/1, 61 motions |
| 8 | Graphic tablet mode, draw | tablet only | **PASS** | tablet 1 tip down / 1 up, pressure 0.23; pointer: no buttons, 2 motions (the pre-existing park-on-range-exit, unchanged) |
| 9 | Duplicate ACTION_DOWN | only one `BTN_LEFT` press | **PASS** | same `seq` sent twice: `BTN_LEFT` 1/1, host logged 1 discard. Without `seq` (older app), DOWN twice 3 ms apart: `BTN_LEFT` 1/1 |
| 10 | Out-of-order events | stale event discarded | **PASS** | sent order hover(1) DOWN(2) UP(3) arriving hover, UP(3), DOWN(2): the late DOWN was discarded (1 discard), nothing pressed; the following clean tap pressed and released once |

Host tests: 149 passed (new: state derivation, `force_release_all`, left-edge guard, duplicate DOWN, lost-UP DOWN, route change
both ways, route check on the clock, range exit that lasts, tip suppression, eraser lift, `SeqFilter`, newest connection).

## Not verified / caveats

* **Krita is not installed.** The double-draw fix is proven where the compositor hands events out (the tablet never reports a
  tip while the pointer owns the tip). Whether Krita itself then draws once, and with pressure on the art route, is untested.
  Real pressure in Krita needs the art route (`focus.json` from the GNOME extension after a re-login, or `uscreen toggle-tablet`).
* The pressure under the tip threshold on the mouse route is a squeezed value (0..150 of 4096). Real pressure needs the tip,
  and libinput ties the tip to pressure; both cannot go to the tablet while the pointer holds the tip.
* Focus was simulated through `focus.json`, the file the extension writes; the extension itself is not loaded in this session.
* The first full-matrix run showed test 1 with zero events on both devices: the app had just been relaunched after the
  disconnect test and was not in the foreground yet. After its foreground ping the same test passes (result above).
* Test 2 needs a pointer hover before the press: with the very first press landing right after the pointer entered the Chrome
  window, Chrome reported the click (`pd=1`) but no selection (twice). Not investigated further; the app always hovers first.
* Tests 9/10: the real app was alive (the daemon relaunches it) while the scripted client ran, so the newest-connection rule
  closed one of the two connections each time; the discards were the intended ones.
* An Android cancel is reported to the host as a pen-up, so the host cannot tell it from a lift; both end in the same release.
* Mouse style is the existing absolute pointer device, not a relative mouse.
