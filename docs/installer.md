# Installer

`scripts/install.sh` installs UScreen from a source checkout or a release tarball.

## Modes

| Command | What it does |
|---|---|
| `./scripts/install.sh` | In a terminal with `whiptail` or `dialog`: a text interface to pick the parts. Otherwise headless. |
| `./scripts/install.sh --tui` | Force the text interface (plain `read` prompts if neither tool exists). |
| `./scripts/install.sh --headless` (`-y`) | No questions, installs everything. For scripts and CI. Needs root or passwordless `sudo`. |
| `./scripts/install.sh --dry-run` | Prints every command it would run, changes nothing. |
| `./scripts/install.sh --plan` | Prints what this system needs (family, packages, EVDI method) and exits. |

Parts: `core` (daemon, desktop app, APK), `menu` (GTK 4 runtime), `extension` (GNOME Shell extension),
`spawnat`, `system` (EVDI driver, udev rules). Leave one out with `--skip-<part>`. `--no-build` skips
`make build` and uses what is already in `target/release/` or `bin/`.

The last lines say what to restart, as text and as `~/.local/share/uscreen/install-result.json`:

```
UScreen-INSTALL-RESULT: relogin=yes reboot=no
```

## When is a restart needed?

Only in these cases, nothing else:

| Situation | Restart |
|---|---|
| GNOME Shell on **Wayland**, and the Shell extension (or spawn-at) was installed or its version changed | log out and in once |
| GNOME on X11, KDE, Cinnamon, wlroots, ... | none (the extension is GNOME-only; X11 can reload the Shell with Alt+F2, `r`) |
| Re-running the installer with the same extension version | none |
| EVDI module is loaded and in use, so it cannot be reloaded with its new settings | reboot |
| Image-based Fedora (Silverblue, Bazzite): packages are layered with `rpm-ostree` | reboot |

## EVDI (virtual display driver)

Only Extended Display mode needs it; Stylus and Touchpad modes work without.

| Family | Method |
|---|---|
| Ubuntu | the distribution's prebuilt signed module for the running kernel (`linux-main-modules-evdi-<kernel>`, or the `linux-modules-evdi-*` metapackage). No compiling. |
| Debian and derivatives | `evdi-dkms`, then upstream source if that does not build. |
| Arch | `evdi-dkms` from the AUR (yay or paru). |
| openSUSE | the `evdi` kernel-module package. |
| Fedora | not packaged: upstream source built with dkms. |
| Bazzite, Nobara | already in the image. |

Rules the installer follows so the package manager never ends up broken: an existing module is
never replaced; a half-installed `evdi-dkms` (its build failed, so every later `apt` call fails) is
purged first, unless dkms has actually built a module from it; a failed build is cleaned up before
the upstream source is tried; a missing EVDI is a warning, never an abort.

## Tests

```
scripts/tests/run.sh                  # 50+ tests, no root, no network, writes only to a /tmp sandbox
scripts/tests/run.sh ubuntu           # only tests with "ubuntu" in the name
scripts/tests/in-containers.sh        # the same suite in Ubuntu 22.04/24.04, Debian 12, Fedora 41, Arch, openSUSE
```

The suite has three layers: platform detection against `os-release` fixtures for every family;
`--plan` and `--dry-run` (which must change nothing); and real runs in a sandbox HOME where `sudo`,
`apt-get`, `dnf`, `modprobe`, `curl`, ... are fakes that only record their calls. That lets every
branch (root, no sudo, offline, half-installed packages, module in use, dev-build spawn-at) be driven
and checked on any machine. CI (`.github/workflows/installer.yml`) runs it in a container per family,
runs ShellCheck, and does a real headless install twice on an Ubuntu runner.

What it cannot check: that a real distribution's package names exist today (the fakes answer for
them), and a real kernel module build. Those need a real machine or VM; run `--plan` there first.

## Optional AI second opinion

`scripts/ci/ai_review.py` sends an install log to the DeepSeek API and prints what it notices. It is
never a gate unless you pass `--strict`. The key comes from `DEEPSEEK_API_KEY` (a CI secret), never
from a file. Before anything is sent, home paths, user names, IP addresses, e-mail addresses, device
serials and token-like strings are replaced; `--print-redacted` shows exactly what would leave the
machine. Without a key it prints "skipped" and exits 0.
