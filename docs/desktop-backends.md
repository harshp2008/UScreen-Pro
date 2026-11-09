<!--
  docs/desktop-backends.md: what a desktop backend can provide, the checklist to add a desktop, each backend's status, and the GNOME backend in detail.
  Owns that guide; the code is in host/src/desktop/.
-->
# Desktop backends: supporting a new desktop

UScreen's host asks one question of the desktop layer (`host/src/desktop/`) instead of checking
which desktop it runs on: *what can this backend do?* A backend answers with a
[`Capabilities`](../host/src/desktop/mod.rs) value and implements only the parts it supports. Every
feature that needs the desktop checks the capability and quietly does nothing when it is missing.

## What a backend can provide

| Capability | Trait methods | Used by |
|---|---|---|
| `focus` | `poll_focus()` | The pen's Dynamic style (experimental, GNOME only; stored as `auto`): Krita/GIMP get the pressure tablet, everything else a mouse |
| `context` | `context_at_pointer(timeout)` | The pen menu: surface under the pointer (app window, shell UI, desktop), window frame, size of the selection and clipboard |
| `menu` | `show_menu(req)`, `wait_menu_choice(req)` | The pen menu, preferred: the desktop draws it itself (no window, nothing in the taskbar, no focus change) |
| `placement` | `focused_window()`, `focus_window(w)`, `spawn_at_pointer(lift, class, command)` | The pen menu, fallback: show the `uscreen-menu` window at the cursor and hand focus back to the previous app |

Key presses (Ctrl+C, ...) are sent through uinput and need no backend.

## Checklist: add a desktop

1. **Pick the signals.** For each capability, find the desktop's way to do it:
   - focus: an extension/script that reports the focused window, a protocol (wlr-foreign-toplevel),
     or an X11 property;
   - context: pointer position, the surface under it, the primary selection and the clipboard
     (report *lengths only*, never the text);
   - placement: a tool or API that opens a window at exact coordinates and can focus a window by id
     (on GNOME: the `spawn-at` command).
2. **Create `host/src/desktop/<name>.rs`.** A struct that implements `DesktopBackend`:
   `name()`, `capabilities()` (set only what really works), `poll_focus()`, and any of the optional
   methods. Reuse `focus_file.rs` if your helper writes `~/.local/share/uscreen/focus.json`.
3. **Pick it in `detect()`** (`desktop/mod.rs`), from `XDG_CURRENT_DESKTOP` / `XDG_SESSION_TYPE`,
   before the `generic` fallback.
4. **Ship the helper**, if there is one (a Shell extension, a KWin script, ...) under `extensions/`,
   and install it from `scripts/install.sh`.
5. **Test.** Unit-test the parsing of whatever your signals return (see `context.rs` and
   `spawn_at.rs`). Then run it on the real desktop: the focus routing (`uscreen debug-dump`,
   `~/.local/share/uscreen/focus.json`), and the pen menu (long-press on text).
6. **Record what you verified** in `experiments_and_findings/` and in the table below. Mark
   anything unverified as experimental.

## Status

| Desktop | Backend | Focus | Context | Placement | Verified |
|---|---|---|---|---|---|
| GNOME Shell (Wayland) | `gnome.rs` | extension (`focus.json`) | extension probe | extension draws the menu; `spawn-at` window as fallback | Ubuntu 24.04, GNOME 46 only |
| Any X11 session | `x11.rs` | `_NET_ACTIVE_WINDOW` | no | no | focus only; not re-tested since the desktop layer was introduced |
| KDE, Sway, Hyprland, others | `generic.rs` | `focus.json` if a helper writes it | no | no | the pen stays on the mouse route; flip it with `uscreen toggle-tablet` |

## The GNOME backend in detail

* **Focus:** the `uscreen-app-lock` extension writes `focus.json` on every focus change.
* **Context:** the daemon writes `~/.local/share/uscreen/context_request`; the extension answers in
  `context.json` (pointer, zone, window frame, `selection_len`, `clipboard_len`, timestamp). The
  daemon accepts the answer only if its timestamp is newer than the request.
* **Menu (preferred):** the daemon writes `menu_request.json` (`id`, `items`, `style`, `timeout_ms`); the extension draws the menu as Shell chrome at the pointer, writes the id to `menu_shown`, then `{id, action}` to `menu_result`. If the extension does not confirm within 400 ms (an older extension) the daemon falls back to the window below.
* **Placement (fallback):** `spawn-at spawn --class <app id> --anchor cursor --pivot center --offset 0 -<lift>
  -- <command>`. The class is required: without it spawn-at does not place a GTK 4 window (measured).
  `spawn-at query windows` and `spawn-at focus --id` give and restore focus.
* The extension only changes after the user logs out and back in on Wayland.
