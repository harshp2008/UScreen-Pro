<!--
  docs/architecture/platform-modularity.md: how platform-specific code is organised after the cleanup, and how to add a desktop,
  a distribution or (what it would take to add) another operating system.
  Owns the final design and the worked example. The audit that led to it, with every tagged place, is
  platform-modularity-plan.md. The shorter, older how-to is ../adding-support.md.
-->
# Platform modularity

## The rule

Code that depends on a desktop, a compositor, a distribution, an init system, a kernel interface or an Android version lives in a
module that is named for that platform, behind a function or trait that every platform implements with the same signature. The
rest of the program asks the interface and never asks "is this GNOME?". When a platform has no implementation the program
keeps running, does less, and says so.

Every such place is tagged with a comment `PLATFORM(<name>): <what it does>`:

```bash
git grep -n "PLATFORM("             # all of them
git grep -n "PLATFORM(hyprland)"    # one platform
```

## What is implemented

| Capability | Interface | Backends | Chosen by |
|---|---|---|---|
| Focus, pointer context, pen menu, window placement | `trait DesktopBackend` + `Capabilities` (`desktop/mod.rs`) | `gnome`, `x11`, `generic` | `desktop::detect()`, at start |
| Enable, place and remove the virtual output | `desktop::display::{enable_output, disable_output}` | `hyprland`, `cosmic`, `gnome` (does nothing: Mutter does it), `x11`, `kde` | `desktop::display::backend()`, per call |
| Pin the virtual input devices to the output; find the output's area | `input/mapping.rs` calls the backend's functions | `hyprland`, `gnome` (also Cinnamon), `x11`, `kde` (COSMIC has none and takes the KDE path, which finds no tool and warns) | `desktop::display::backend()` |
| Interface scale of the virtual display | `desktop::scale::{supported, apply, keep}` | `gnome`, `hyprland`, `kde` (X11 recognised: nothing to set) | `desktop::scale::backend()` |
| On-screen keyboard suppression | `desktop::kde::osk::{disable, restore}` | `kde` only | direct call, KDE session check inside |
| Distribution packages and the EVDI module | `scripts/lib/distros/<family>.sh` (interface in `_template.sh`) | `debian`, `fedora`, `arch`, `suse`, `unknown` | `us_distro_family` from `/etc/os-release` |
| GNOME Shell versions | `extensions/uscreen-app-lock/compat/` | the versions in `compat/versions.js` | `Shell.version` at run time |
| Android versions | `Build.VERSION.SDK_INT` branches inside `PlatformSupport.kt`, `PowerSupport.kt`, `MainActivity.kt` and friends | API levels | run time |

### The display dispatcher

`host/src/desktop/display.rs` is the pattern to copy:

```rust
pub enum DisplayBackend { Hyprland, Cosmic, Gnome, X11, Kde }
pub fn backend() -> DisplayBackend;                         // first backend whose active() is true
pub fn connector_names(card: Option<u32>) -> Vec<String>;   // EVDI connectors of this tablet, from sysfs
pub async fn enable_output(card: Option<u32>, position: Position);
pub async fn disable_output(card: Option<u32>);
```

Each `desktop/<name>/display.rs` offers the same functions:

```rust
pub fn active() -> bool;                                          // cheap, no side effects
pub async fn enable_output(names: &[String], position: Position);
pub async fn disable_output(names: &[String]);
```

plus whatever else that desktop needs for input mapping (`map_devices`, `output_area`, ...). Two things to know:

* **Order matters and is written once**: Hyprland, COSMIC, GNOME, X11, then KDE. `Kde` is the fall-through: on a desktop none of
  the others recognise, the KDE functions run, find no `kscreen-doctor`, and the feature does nothing but log. That is how it
  behaved before the dispatcher existed.
* **A combination that cannot happen is not handled.** The old input-mapping code tested X11 before COSMIC and the enabling code
  tested COSMIC before X11. COSMIC has no X11 session, so the two orders never disagree on a real system; the dispatcher uses one.

### What is deliberately not abstracted

Where only one implementation exists, the seam is a doc comment, not a trait:

| Seam | Today | Where it is documented |
|---|---|---|
| Input injection | uinput (Linux) only: `input/uinput/` | `PLATFORM(uinput)` tags |
| Capture source | EVDI + `evdi_helper` (Linux) only | `host/evdi/`, `PLATFORM(evdi)` tags |
| Service control and autostart | `systemctl --user` in `src-tauri/src/daemon.rs`, `daemon/tray.rs`, `build-all.sh`, the Makefile, the installer | plan, group 9 |
| Compositor health | GNOME Mutter probes in `daemon/guard/probes.rs` | plan, group 7 |
| Paths | XDG-style `~/.config/uscreen`, `~/.local/share/uscreen`, `~/.local/bin`, repeated in about a dozen places | plan, group 9 |

Adding a second implementation of any row is the moment to introduce its trait.

## How to add a new desktop: a worked example (Sway)

This is a sketch for the shape of the work, not code that was run on Sway. Sway answers to `swaymsg`.

**1. Make the directory.** `host/src/desktop/sway/mod.rs` and `host/src/desktop/sway/display.rs`. `mod.rs` needs a module doc and
one tag; it implements `DesktopBackend` only when Sway can answer focus questions (it can: `swaymsg -t get_tree`), otherwise it
stays empty and `generic` serves.

```rust
//! Sway: the virtual output through `swaymsg`.
// PLATFORM(sway): output enable and placement through swaymsg; input mapping with `input ... map_to_output`
pub mod display;
```

**2. Write `display.rs` with the standard three functions.**

```rust
//! Virtual display on Sway.

use crate::config::Position;

// PLATFORM(sway): swaymsg is the only way to talk to Sway from outside the compositor
async fn swaymsg(args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("swaymsg").args(args).output().await.ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A Sway session with `swaymsg` installed.
pub fn active() -> bool {
    std::env::var_os("SWAYSOCK").is_some() && crate::desktop::command_exists("swaymsg")
}

/// Enable the first of `names` and put it on the `position` side of the other outputs.
pub async fn enable_output(names: &[String], position: Position) {
    // swaymsg -t get_outputs -r  -> JSON with each output's name, rect and active flag;
    // swaymsg output NAME enable pos X Y
    // Skip the call when the output is already on and already there (a repeated call makes Sway reconfigure every output).
    let _ = (names, position);
}

/// Switch the outputs in `names` off.
pub async fn disable_output(names: &[String]) {
    for n in names {
        let _ = swaymsg(&["output", n, "disable"]).await;
    }
}
```

**3. Register it in `desktop/display.rs`** (four edits, all in that file):

```rust
pub enum DisplayBackend { Hyprland, Cosmic, Sway, Gnome, X11, Kde }   // 1. a variant
// 2. in backend():   else if super::sway::display::active() { DisplayBackend::Sway }
// 3. in enable_output():   DisplayBackend::Sway => super::sway::display::enable_output(&names, position).await,
// 4. in disable_output():  DisplayBackend::Sway => super::sway::display::disable_output(&names).await,
```

and `pub mod sway;` in `desktop/mod.rs`. The compiler now lists every `match` on `DisplayBackend` that needs an arm
(`input/mapping.rs`, `cli/doctor/display.rs`, `cli/doctor/system.rs`).

**4. Input mapping.** Add `map_devices_on_sway` to `input/mapping.rs` next to the Hyprland one (it needs the device names from
`DeviceIdentity`), calling `swaymsg input <identifier> map_to_output <output>` for the touch and pen devices, and the arm in
`map_devices_to_output`. Return the output name on success and `None` with a warning when Sway lists no such output.

**5. Scale** (optional): `desktop/sway/scale.rs` with `apply(connectors, percent) -> bool` using `swaymsg output NAME scale S`,
then a variant in `scale::Backend`, `backend()` and `apply()`.

**6. Tests.** Parsing and command building are pure functions: unit test them with a captured `get_outputs` JSON in
`desktop/sway/display/tests.rs`, as the other backends do. Add the desktop to `docs/compatibility.md` as "not yet confirmed on
real hardware" until someone reports a run.

**7. Docs.** One row in the table of `docs/desktop-backends.md` and a line in `desktop/mod.rs`'s module doc. Search for the old
count of backends with `git grep -n "PLATFORM("` to see what else names the list.

## How to add a distribution

Copy `scripts/lib/distros/_template.sh` to `scripts/lib/distros/<family>.sh`, fill in the seven functions, add a fixture
`scripts/tests/fixtures/os-release.<name>` and run `scripts/tests/run.sh`. `unknown.sh` is what an unlisted family gets: the files
are copied and a warning says dependencies must be installed by hand.

## How to add a GNOME Shell version

Add the version to `extensions/uscreen-app-lock/compat/versions.js` and `metadata.json` (the check
`node extensions/uscreen-app-lock/tests/check-versions.mjs` fails when they disagree) and put any code that differs in
`compat/`.

## What another operating system would take

The host is Linux in three places that cannot be hidden behind a function: the virtual display (EVDI is a Linux kernel
module), the input devices (uinput) and the service manager (systemd). A Windows or macOS host is therefore three new
implementations, not a new backend file: a capture source with the same "NV12 frames into a FIFO" contract as `evdi_helper`, an
injector that can present a pen and a multi-touch device (neither OS lets a user program create one without a driver), and a
service manager behind the `ServiceManager` seam described in the plan. The desktop app and the tablet app need no change for
the video and input protocols; the desktop app's `systemctl` calls and Linux paths are the parts to abstract first. Until that is
a goal, the seams are documented rather than built.

## Fallback behaviour, summarised

| Missing | What happens |
|---|---|
| No display backend for the desktop | The output is left as the compositor made it; devices are not mapped, so touch and pen address the whole desktop; a warning says so |
| No `DesktopBackend` (generic) | The pen uses the route chosen in settings; no menu, no focus-based routing |
| No scale backend | The setting does nothing and the settings page says the desktop cannot do it |
| Tool not installed (`kscreen-doctor`, `xrandr`, `hyprctl`, ...) | That backend's `active()` is false or its calls return nothing; `uscreen doctor` names the tool |
| Unknown distribution | Files are copied; dependencies by hand |
| Unsupported GNOME Shell version | The extension logs which version it found and what is known about it |
