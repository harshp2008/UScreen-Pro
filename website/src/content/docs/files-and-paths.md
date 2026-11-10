---
title: "Files and paths"
---

Where UScreen Pro reads and writes. Nothing is written outside your home directory except the system files of the `system` install part.

## Your settings and data

| Path | What it is |
| :--- | :--- |
| `~/.config/uscreen/config.toml` | all settings; read and written by the daemon, the desktop app and (through the daemon) the tablet app. See [Configuration](/UScreen-Pro/configuration/) |
| `~/.local/share/uscreen/` | the data directory shared with the GNOME Shell extension and the command line |
| `~/.local/share/uscreen/uscreen.pid` | process id of the running daemon |
| `~/.local/share/uscreen/install-result.json` | what the last install asked you to restart |
| `~/.local/share/uscreen/app_lock.json` | the app-lock settings the daemon writes for the extension |
| `~/.local/share/uscreen/focus.json` | the focused app, written by the extension |
| `~/.local/share/uscreen/context_request`, `context.json` | a request for, and the answer about, what is under the pen (for the pen menu) |
| `~/.local/share/uscreen/menu_request.json`, `menu_shown`, `menu_result`, `menu_close` | the pen menu conversation with the extension |
| `~/.local/share/uscreen/toggle_request` | written by Ctrl+Alt+P or `uscreen toggle-tablet` |
| `~/.local/share/uscreen/pen_dump.txt` | written by `uscreen debug-dump` |
| `$XDG_STATE_HOME/uscreen/guard.json` (default `~/.local/state/uscreen/guard.json`) | the compositor-safety record read at start; cleared by `uscreen reset-guard` |

## Runtime files

`$XDG_RUNTIME_DIR/uscreen/` (a per-user directory with mode 0700; the files in it are 0600) holds the session token (`token`), the control socket (`control.sock`) and the capture FIFO. They disappear when you log out.

## Installed program files

By the installer: `~/.local/bin/uscreen`, `uscreen-pro-gui`, `uscreen-menu`, `evdi_helper`, the desktop entry `~/.local/share/applications/uscreen-pro.desktop`, the systemd user unit `~/.config/systemd/user/uscreen.service` and the extension in `~/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/`.

## System files (the `system` part)

| Path | Purpose |
| :--- | :--- |
| `/etc/modprobe.d/uscreen-evdi.conf` | `options evdi initial_device_count=2` |
| `/etc/modules-load.d/uscreen.conf` | loads `evdi` and `uinput` at boot |
| `/etc/udev/rules.d/60-uscreen-uinput.rules` | lets the logged-in user open `/dev/uinput` |
| `/etc/udev/rules.d/99-uscreen-evdi.rules` | an EVDI rule shipped in `packaging/` |

## Sources

- `SECURITY.md` ("What the installer and packages change", "How to uninstall completely")
- `host/src/config/handler.rs`, `host/src/daemon/runtime.rs`, `host/src/daemon/guard/boot_loop.rs` (paths)
- `host/src/desktop/mod.rs` (`data_file`), `host/src/input/ghost/control.rs`
- `scripts/install.sh` (`install_files`, `install_extension`, `system_setup`)
