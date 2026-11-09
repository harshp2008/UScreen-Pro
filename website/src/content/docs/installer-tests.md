---
title: "Installer tests"
---

`scripts/tests/run.sh` has three layers and runs in a sandbox under `/tmp` where `sudo`, `apt-get`, `dnf`, `pacman`, `zypper`, `modprobe`, `curl` and the other system tools are fakes that only record their calls. The suite never changes the real system, and it removes its sandbox on exit (never a mount point).

| Layer | What it checks |
| :--- | :--- |
| 1. platform detection | the family, EVDI method, packages, desktop, session and restart rules, against `os-release` fixtures of 16 distributions; and that every `scripts/lib/distros/<family>.sh` implements the whole interface, claims its own IDs and names a known EVDI method |
| 2. plan and dry run | `--plan` works on every fixture; `--dry-run` changes nothing and prints the commands |
| 3. real runs with fakes | the files installed, the extension modules (and that stale ones are removed), the result line and `install-result.json`, each family's package manager, root versus `sudo`, every EVDI case, `spawn-at`, and the text interface |

Run one subset by name: `scripts/tests/run.sh ubuntu`. `scripts/tests/in-containers.sh` runs the same suite in Docker containers of Ubuntu 22.04 and 24.04, Debian 12, Fedora 41, Arch and openSUSE; CI does the same.

What it cannot check: that a real distribution's package names exist today, and a real kernel module build.

## Sources

- `scripts/tests/run.sh`, `scripts/tests/in-containers.sh`, `scripts/tests/fixtures/`
- `.github/workflows/installer.yml`, `docs/installer.md`
