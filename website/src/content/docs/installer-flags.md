---
title: "Installer flags"
---

`scripts/install.sh` installs UScreen Pro from a source checkout or a release tarball.

## Modes

| Flag | What it does |
| :--- | :--- |
| none | In a terminal with `whiptail` or `dialog`, a text interface to pick the parts; otherwise headless |
| `--tui` | Force the text interface (plain `read` prompts if neither tool exists) |
| `--headless`, `-y`, `--yes` | No questions; installs everything. Needs root or passwordless `sudo` |
| `--dry-run` | Prints every command it would run and changes nothing |
| `--plan` | Prints what this system needs (family, desktop, session, image based, packages, EVDI method) and exits |
| `-h`, `--help` | Prints the options |

## Parts

`core` (daemon, desktop app, APK, binaries), `menu` (the GTK 4 runtime), `extension` (the GNOME Shell extension), `spawnat` (the window-placement tool for the fallback pen menu) and `system` (the EVDI driver, the udev rules, the modprobe and module-load files). Leave one out with `--skip-core`, `--skip-menu`, `--skip-extension`, `--skip-spawnat` or `--skip-system`. `--no-build` uses what is already in `target/release/` or `bin/` instead of running `make build`.

## Exit codes

| Code | Meaning |
| :--- | :--- |
| 0 | done (also for `--help` and `--plan`) |
| 2 | needs root: passwordless `sudo` is missing in a headless run, or there is no `sudo` and you are not root |
| 64 | an option the installer does not know |

## The result line

The last lines are `UScreen-INSTALL-RESULT: relogin=yes|no reboot=yes|no`, repeated in `~/.local/share/uscreen/install-result.json` as `{"relogin": ..., "reboot": ...}`.

## Sources

- `scripts/install.sh` (the header comment, `parse_args`, `report_result`)
- `scripts/tests/run.sh` (`t_unknown_option_exits_64`, `t_non_root_without_passwordless_sudo_fails_headless`)
- `docs/installer.md`
