<!--
  docs/cleanup/proposed-layout.md: the target directory layout proposed for phase 5 of the cleanup, awaiting approval.
  Owns the proposal only. Once approved and done, docs/codebase-changes.md holds the old-to-new path table.
-->

# Proposed layout (phase 5): awaiting approval

Goal: adding or debugging a desktop environment, a GNOME Shell version or a distribution is one small,
isolated change. Nothing below changes behaviour; every move is checked by the compiler, the tests and
`grep` for the old paths.

## 1. Desktop environments: one directory each

Today the code for one desktop is spread over `host/src/desktop/<name>.rs` (what the pen menu and focus need)
and `host/src/display/<name>.rs` (how the virtual monitor is enabled, placed and mapped), plus
`host/src/daemon/osk.rs` (KDE on-screen keyboard).

```text
host/src/desktop/
├── mod.rs              DesktopBackend trait, Capabilities, detect()          (unchanged)
├── context.rs          pointer-context types shared by backends              (unchanged)
├── focus_file.rs       reader of focus.json for helpers that write it        (unchanged)
├── generic/mod.rs      fallback for every desktop without a backend          (was desktop/generic.rs)
├── x11/
│   ├── mod.rs          focus via _NET_ACTIVE_WINDOW                          (was desktop/x11.rs)
│   └── display.rs      RandR: enable, place, map input devices               (was display/x11.rs)
├── gnome/
│   ├── mod.rs          focus, context, menu, placement via the extension     (was desktop/gnome.rs)
│   ├── display.rs      Mutter D-Bus + dconf mapping                          (was display/gnome.rs)
│   └── spawn_at.rs     window placement with the spawn-at tool               (was desktop/spawn_at.rs)
├── kde/
│   ├── mod.rs          status and what exists for KDE
│   ├── display.rs      KWin D-Bus                                            (was display/kwin.rs)
│   └── osk.rs          on-screen keyboard policy                             (was daemon/osk.rs)
├── hyprland/
│   ├── mod.rs
│   └── display.rs      hyprctl                                               (was display/hyprland.rs)
└── cosmic/
    ├── mod.rs
    └── display.rs      cosmic-randr                                          (was display/cosmic.rs)

host/src/display/       what is NOT desktop specific
├── mod.rs
├── edid.rs             generated EDID                                        (unchanged)
└── vdisplay.rs         EVDI card and connector discovery in sysfs           (unchanged)
```

Adding a desktop = one new directory `host/src/desktop/<name>/`, one line in `desktop/mod.rs`
(`detect()`), one row in the status table. Sway is not a directory yet because it has no code
(it uses `generic`); the guide says how to add it.

Not changed (and stated so you can object): `host/src/input/mapping.rs` and
`host/src/cli/doctor/display.rs` still choose the display backend with an `if backend::active()` ladder.
Replacing the ladders with a `DisplayBackend` trait is a design change, not a move, so it is **not** part of this
proposal. Say so if you want it.

## 2. GNOME Shell extension: a compatibility layer

```text
extensions/uscreen-app-lock/
├── extension.js        thin entry: enable()/disable() only; wires the modules
├── metadata.json       the list of supported Shell versions (the one place; machine-read by GNOME)
├── stylesheet.css, schemas/…
├── lib/
│   ├── dataFiles.js    ~/.local/share/uscreen/ helpers
│   ├── appLock.js      keep chosen apps off the virtual display
│   ├── penRouting.js   focus.json and the Ctrl+Alt+P hotkey
│   ├── contextProbe.js context_request → context.json
│   └── penMenu.js      the long-press menu drawn in the Shell
├── compat/
│   ├── index.js        picks the shim for the running Shell (Config.PACKAGE_VERSION)
│   ├── versions.js     SUPPORTED = { 45: …, 46: …, 47: … } and the facts that differ
│   └── shell-46.js …   one small file per Shell version that needs special handling
└── tests/check-versions.mjs   node check: metadata.json and compat/versions.js agree
```

Adding a GNOME version = add the number to `metadata.json`, add a row in `compat/versions.js`, add a
`shell-<n>.js` only if something differs. The loadable-in-Shell parts cannot be tested here (a Shell
extension reloads on Wayland only after logout), so I will only run `node --check` and the version check.

**Finding to decide:** `metadata.json` lists Shell 42–44, but `extension.js` uses the ES-module format of GNOME 45+,
which those versions cannot load. I will keep the list as it is unless you say otherwise.

## 3. Installer: one file per distribution family

```text
scripts/
├── install.sh                  steps and modes only (--headless, --tui, --dry-run, --plan; UScreen-INSTALL-RESULT:)
└── lib/
    ├── platform.sh             detection (desktop, session, immutable) and restart rules; loads distros/*.sh
    ├── evdi.sh                 EVDI steps shared by families (module check, dkms-from-source, activate)
    ├── tui.sh                  unchanged
    └── distros/
        ├── debian.sh           Ubuntu, Debian, Mint, Pop!_OS: IDs, packages, EVDI strategy, gtk4, prebuilt module
        ├── fedora.sh           Fedora, RHEL-likes, Nobara, Bazzite: rpm-ostree, RPM Fusion, EVDI from source
        ├── arch.sh             Arch, Manjaro, EndeavourOS, CachyOS: pacman, evdi-dkms from the AUR
        ├── suse.sh             openSUSE, SLES: zypper, evdi package
        ├── unknown.sh          the fallback: warn and install only files
        └── _template.sh        copy this to add a distribution (not loaded)
```

Every family file defines the same functions: `distro_ids`, `distro_core_packages`, `distro_install_deps`,
`distro_install_gtk`, `distro_evdi_strategy`, `distro_evdi_ensure` and (optional) `distro_layers_packages`.
`platform.sh` finds the family by matching `ID`/`ID_LIKE` against each file's `distro_ids`, so a new
distribution is one new file plus an `os-release` fixture and tests. The existing 53 tests stay and move to
the new interface; I add a test that every family file implements the whole interface, and one per file.

## 4. Docs and site (phases 6 and 7)

```text
docs/                  stays the source of the project documents (README, installer, desktop backends, …)
docs/adding-support.md new: how to add a distro / a desktop / a GNOME version
website/               new: Starlight site (see the questions below for its URL)
.github/workflows/docs.yml   new: GitHub Pages workflow (prepared, not run)
```

## Questions

1. Approve sections 1–3 as drawn, or change them?
2. Files left alone until you decide: `host/evdi/evdi_capture_test.c`, `host/evdi/evdi_grab_test.c`,
   `host/evdi/evdi_test2.c`, `android/.../ui/MorphButton.kt`, `scripts/test-gen-edid.py`, and the boilerplate
   `uscreen-pro-gui/README.md` (the unmodified Tauri template text). Delete, keep, or replace?
3. The existing GitHub Pages files in `docs/` (`index.html`, `robots.txt`, `sitemap.xml`, `llms.txt`,
   `.nojekyll`) point at `majmichu1.github.io/UScreen`. Keep them next to the new site, move them to
   `docs/legacy-site/`, or delete them when the new site goes live?
4. Site URL and base path. Recommendation: `https://harshp2008.github.io/UScreen-Pro/` with base `/UScreen-Pro/`.
