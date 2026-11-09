<!--
  docs/adding-support.md: how to add support for a new Linux distribution, a new desktop environment or a new GNOME Shell version.
  Owns the step lists and the files each step touches. The reference for what a desktop backend can do is
  docs/desktop-backends.md; for what the installer does, docs/installer.md.
-->

# Adding support

Each of the three is one small change in one place. None needs a change to the files that already work.

| You want to add | Where it lives | Files you add or edit |
|---|---|---|
| a Linux distribution | `scripts/lib/distros/` | one new `<family>.sh`, one `os-release` fixture, a few test lines |
| a desktop environment | `host/src/desktop/` | one new directory, one line in `detect()`, one row in the status table |
| a GNOME Shell version | `extensions/uscreen-app-lock/` | one number in `metadata.json`, one row in `compat/versions.js`, a `compat/shell-<n>.js` only if behaviour differs |

Nothing on this page is claimed to work until it has been run on the real system. Say what you verified
in the pull request and mark the rest experimental.

## A distribution

A *family* is a group of distributions that share a package manager and package names (Debian and Ubuntu are
one family; Arch and Manjaro are one). If your distribution is already in a family, **add its `ID` to that
family's `<family>_ids` line and stop**. Add a new file only for a new family.

1. **Copy the template.** `cp scripts/lib/distros/_template.sh scripts/lib/distros/<family>.sh`, then replace
   `FAMILY` with the family name in every function name. The name of the file is the name of the family. The file
   starts with a comment saying what it owns.
2. **Fill in the functions.** All of them are required (a test fails if one is missing):

   | Function | Must do |
   |---|---|
   | `<family>_order` | echo a number; families are tried against os-release from the lowest. Only matters if two families could claim one system |
   | `<family>_ids` | echo the `ID` and `ID_LIKE` values of the family |
   | `<family>_core_packages` | echo the package names the daemon needs (also shown by `--plan`) |
   | `<family>_evdi_strategy` | echo `prebuilt`, `dkms`, `package`, `source` or `bundled` |
   | `<family>_install_deps` | install those packages with `run $SUDO ...`; set `US_PKG_LAYERED=1` if they only apply after a reboot |
   | `<family>_install_gtk` | install the GTK 4 runtime of the fallback pen menu; return non-zero if it cannot |
   | `<family>_evdi_install` | get the EVDI module the family's own way; return 0 on success, non-zero to fall back to building upstream source |
   | `<family>_evdi_prepare` | optional: runs first in the EVDI step (Debian repairs a half-configured package here) |

   Use `run` for everything that changes the system, so `--dry-run` prints it instead of doing it. Use `info` and
   `warn` for messages.
3. **Check the package names on the real distribution** (a container is enough): ask the package manager
   what it has. Do not copy names from another distribution.
4. **Add a fixture.** Copy `/etc/os-release` from the distribution to
   `scripts/tests/fixtures/os-release.<name>`.
5. **Add tests** in `scripts/tests/run.sh`: the fixture in `t_family_for_every_fixture` and
   `t_evdi_strategy_per_distro`, one test that the install calls the right package manager (see
   `t_arch_uses_pacman`), and one for anything unusual in your `_evdi_install`. The interface test
   `t_every_family_file_implements_the_whole_interface` already covers your new file.
6. **Run** `scripts/tests/run.sh` (no root, no network, nothing outside a sandbox). To run it in a container of
   the new distribution add it to the matrix of `.github/workflows/installer.yml`.
7. **Document it**: a row in `docs/compatibility.md` (what you tried) and in the installer reference.

## A desktop environment

Everything one desktop needs sits in one directory, `host/src/desktop/<name>/`:

| File | What it holds |
|---|---|
| `mod.rs` | the `DesktopBackend` implementation (optional; without it the desktop uses `generic`). Set only the capabilities that really work |
| `display.rs` | enabling, placing and mapping the virtual monitor and the tablet's input devices on this compositor |
| other files | anything else that belongs to the desktop only (`spawn_at.rs` for GNOME, `osk.rs` for KDE) |

1. **Pick the signals** for each capability (focus, pointer context, window placement, menu). See the table in
   [desktop-backends.md](desktop-backends.md).
2. **Create the directory** and a `mod.rs` that starts with a `//!` comment saying what the backend does and
   what it does not. Implement `DesktopBackend` from `host/src/desktop/mod.rs`.
3. **Register it:** `pub mod <name>;` and one arm in `detect()` in `host/src/desktop/mod.rs`.
4. **Wire the display code**: the compositor-specific calls that enable the virtual monitor and map devices are
   chosen by `active()` checks in `host/src/capture/display.rs`, `host/src/input/mapping.rs` and
   `host/src/cli/doctor/`. Add your `active()` there (these ladders are the one place that still lists every desktop).
5. **Ship the helper**, if the desktop needs one (a Shell extension, a script), under `extensions/`, and install it
   from `scripts/install.sh`.
6. **Test**: unit-test the parsing of what your signals return, then try it on the real desktop. Record what you
   verified in `experiments_and_findings/` and in the status table.

## A GNOME Shell version

The extension runs inside `gnome-shell`; it cannot be started outside a real session, and on Wayland a changed
extension loads only after a log out. So the checks that can run anywhere are syntax and the version lists.

1. **Add the number** to `shell-version` in `extensions/uscreen-app-lock/metadata.json`. That list is what GNOME reads.
2. **Add a row** to `SHELL_VERSIONS` in `extensions/uscreen-app-lock/compat/versions.js` with its status:
   `tested` (you ran it), `declared` (named but not run) or `unloadable` (named but cannot load).
3. **Check**: `node extensions/uscreen-app-lock/tests/check-versions.mjs` fails if the two lists differ, and
   `node --check` on each file checks syntax. CI runs both.
4. **Only if something behaves differently** on that version: put the difference in the `compat/` layer, not in
   `lib/`. The existing example is `compat/monitors.js`, which finds a monitor by connector name with each API
   that exists, newest first. Add `compat/shell-<n>.js` for a larger difference and export it through
   `compat/index.js`. The modules in `lib/` must never test the Shell version themselves.
5. **Bump `version`** in `metadata.json` when any extension file changes: the installer compares it to decide
   whether the user must log out once.
6. **Try it** on that Shell, log out and in, and watch `journalctl --user -u gnome-shell` (or
   `journalctl /usr/bin/gnome-shell`) for `[uscreen-app-lock]` lines. The first line says which Shell it found and what is
   known about it.
