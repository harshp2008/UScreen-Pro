---
title: "Concepts"
---

## The three modes

The tablet is in one mode at a time. The mode is a setting (`input_mode`): a global default, which a device can override.

| Mode | The tablet is | Needs EVDI | Stored as |
| :--- | :--- | :--- | :--- |
| **Extended Display** | a second monitor: the daemon creates a virtual display, encodes it and streams it; touch and pen come back | yes | `display` |
| **Touchpad** | a laptop touchpad: fingers move the pointer, a tap clicks, two fingers scroll, three fingers swipe between workspaces; nothing is streamed | no | `touchpad` |
| **Stylus** | a pen tablet for the computer's own screen: nothing is streamed, the pen drives the screen you are looking at | no | `stylus` |

In Touchpad mode the gestures are recognised on the tablet by a port of the libgestures rules (tap to click, tap and drag, palm rejection) and sent to the computer as plain mouse, scroll and gesture messages. The Android app ignores finger touches in Stylus mode, and the pen works in every mode.

## Pen styles

In Stylus mode, and for the pen in Extended Display mode, the pen style decides what the computer sees.

| Style | Stored as | What it does |
| :--- | :--- | :--- |
| **Dynamic** (experimental, GNOME only) | `auto` | the pen is a graphics tablet with pressure and tilt in drawing programs (Krita, GIMP and the list `stylus_art_apps`) and a mouse everywhere else; the computer keeps a visible cursor under the pen. It needs the GNOME Shell extension to know which app has focus |
| **Mouse** | `mouse` | the pen is always a mouse pointer; a side button can be right click, middle click or nothing |
| **Graphics tablet** | `tablet` | the pen is always a pressure and tilt tablet |

On the computer Ctrl+Alt+P (the GNOME extension) or `uscreen toggle-tablet` flips the route of the focused app between tablet and mouse.

Dynamic mode has a state machine for each stroke: a small zone around the first touch swallows wobble so a tap stays a tap, a pen that rests without moving is parked so it stops taking the cursor from a real mouse, and a press held still for about half a second opens the pen menu. Its details are in [Behaviour notes](/UScreen-Pro/project/behavior-notes/) and the experiment records 95 to 101.

## The pen menu

A press held still opens a small menu at the pen: Cut, Copy, Paste, Select all and Undo, offered only where they make sense (a selection or clipboard text, and not in a window's title strip or on the desktop). On GNOME the extension draws it inside the Shell, so it is not a window and does not change focus. Where it cannot, the daemon shows a small GTK window (`uscreen-menu`) at the pointer, placed with the `spawn-at` tool, and hands focus back. The menu is a setting (`pen_menu`) with its own hold time and style (text or icons).

## Settings and who wins

The desktop app and the tablet app both edit `config.toml`, and both a global value and a per-device value can exist. The newest change wins: a global change clears the per-device override of that setting, and a device change sets it for that device only. The desktop window re-reads the file every 1.5 seconds and merges what the tablet changed, without losing what you are editing and have not saved. See [Settings and protocol](/UScreen-Pro/settings-and-protocol/).

## Sources

- `uscreen-pro-gui/src/App.tsx` (`mergeConfig`), `uscreen-pro-gui/src-tauri/src/config.rs` (the last-change-wins tests)
- `host/src/input/ghost/` (`auto.rs`, `route.rs`), `host/src/input/stillness.rs`, `host/src/input/menu/`
- `android/app/src/main/java/com/uscreen/HostLink.kt` (the mode and style names), `TouchpadEngine.kt`, `TapToClick.kt`, `PalmFilter.kt`
- `extensions/uscreen-app-lock/lib/penMenu.js`
