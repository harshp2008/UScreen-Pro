# 39. Frameless window, light/dark MD3, daemon controls, bug fixes

Test rig: the Tauri window is native Wayland, which `xdotool` cannot see, so verification launched the deployed binary with `GDK_BACKEND=x11` (XWayland) and drove it with `xdotool`, capturing with `import -window`. The display runs at 125% scaling, so CSS widths are 80% of screenshot pixels.

Research: Tauri 2 "window customization" page (permissions, drag region, window API). The M3 Expressive blog page is script-rendered and returned no text, so colour/shape/type decisions come from existing M3 knowledge, not a fresh read.

## 1. Frameless window and title bar
- **Hypothesis:** `decorations: false` plus a `data-tauri-drag-region` bar and four `core:window:allow-*` permissions gives a working custom frame.
- **Method:** edit `tauri.conf.json`, `capabilities/default.json`, new `TitleBar.tsx`; relaunch; click each control.
- **Raw log:**
  ```
  _MOTIF_WM_HINTS = 0x2, 0x0, 0x0, 0x0, 0x0        (no decorations)
  maximize click  -> 0,40 1920x1040  _NET_WM_STATE_MAXIMIZED_HORZ, _MAXIMIZED_VERT
  restore click   -> 100,140 1024x757
  minimize click  -> _NET_WM_STATE_HIDDEN / WM_STATE Iconic
  drag-region double-click -> maximized (same handler as the drag attribute)
  close click     -> pgrep uscreen-pro-gui: 1 -> 0
  ```
- **Conclusion:** minimize, maximize/restore, close and double-click-to-maximize verified. **Press-and-drag to move was not confirmed**: synthetic XTEST drags (two attempts, 6 and 30 steps) left the window at 100,140. The attribute and `allow-start-dragging` permission are present in the binary, so this is likely a synthetic-input limit under Mutter/XWayland; it needs one manual drag on the real desktop.

## 2. Daemon controls and theme
- **Hypothesis:** systemd is the right owner, because `uscreen.service` exists with an override pointing at the repo build and `ExecStop=uscreen stop`.
- **Method:** `launch_daemon` starts the unit (binary fallback, detached); new `stop_daemon` stops it (`uscreen stop` fallback); both wait up to 8 s for the PID file. Sidebar card shows dot, PID and a Start/Stop button. Clicked both in the GUI.
- **Raw log:**
  ```
  before: unit=inactive pid=
  after start: unit=active pid=227522   227522 .../target/release/uscreen start
  after stop:  unit=inactive pidfile=   procs=0
  ```
- **Theme:** Tailwind v4 has no `tailwind.config.js`; the equivalent is `@custom-variant dark (&:where(.dark, .dark *))` in `styles.css`. Full light and dark token sets live in `:root`/`.dark`. Choice stored in `localStorage` (`uscreen-theme`), applied before first paint. Verified: toggle flips both ways; dark survived a restart.

## 3. Bugs
| Item | Finding | Fix |
|---|---|---|
| Dead space on maximize | content capped at `max-w-2xl`, left-aligned | `mx-auto w-full max-w-5xl` wrapper, views full width (see `39_dark_maximized.png`) |
| Quality 17 vs "18 default" | the code default is 18; **`~/.config/uscreen/config.toml` has `quality = 17`** | 5 named stops (14/18/22/26/30 sent to the backend); stored values snap to the nearest label for display only, so 17 shows "Balanced" and is not rewritten until moved |
| App locking | bare text input | switch, default off, field only when on; off stashes and clears the list, on restores it |
| Fallback Wi-Fi address | global field applied to every device | input removed; per-device address stays in device settings and the wizard |
| System Health clipping | table cells had no left padding, cards overflowed | card grid with `auto-fit`, wrapped values, padded scrollable table; verified at 1024 px and maximized |
| Wizard paragraph | one long Note | `ul.list-disc.list-inside` |

Caveat: the host still reads `wifi_address` from the config as a single daemon-wide fallback. The user's file still contains `wifi_address = "10.238.4.100:5555"` and a legacy `[tablets."10.238.4.100:5555"]` profile. I did not delete either; clearing the global key in the file is the remaining step.

## 4. Expressive styling
Indigo primary for active states and buttons, teal secondary for switches, segmented selection and menu checks, tonal surface containers (surface, low, container, high, highest) for depth, bold coloured section headers, buttons and nav items morph 4px to 8px on hover. Radius stays at or below 8px; no gradients or shadows.

## Not verified
Real tablet flows (unchanged from doc 38), Android UI on a device, the press-and-drag move, native-Wayland rendering (tested through XWayland).
