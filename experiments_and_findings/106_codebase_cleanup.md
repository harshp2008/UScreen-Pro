# 106: Codebase cleanup, platform audit and display dispatcher

Date: 2026-11-12. Branch `chore/codebase-cleanup`, from `feat/settings-ui-overhaul` at `ed97427`. Host Ubuntu GNOME 46 on Wayland;
tablet SM-X616B connected over USB; the phone was offline.
Related: `docs/codebase-changes.md` (the earlier cleanup), record 105.

## Question

Can the tree be made easier to read and to port without changing what it does, and where does it depend on one platform?

## What was done (one commit per part)

| Part | Result |
|---|---|
| Baseline | all checks pass except `cargo fmt --check` (5 hunks in the Tauri crate); written down in `docs/cleanup/baseline.md` |
| Comments | the tree was already commented by the earlier cleanup; the gaps found by a script (header comments on seven GUI files, undocumented constants, a misplaced KDoc, test fixtures) were filled |
| Dead code | almost none: two removals and five merged copies of helpers, with evidence in `docs/cleanup/removed.md` |
| Splits | App.tsx, PowerSettings.tsx, AddDeviceWizard.tsx (types only), six inline test modules, `kde/display.rs`, the pen tilt maths |
| Platform audit | 134 `PLATFORM(<name>)` tags and `docs/architecture/platform-modularity-plan.md` |
| Reorganisation | `desktop/display.rs` dispatcher, GUI folders `wizard/ power/ shell/`, Android packages `input/ video/` |
| Review | `docs/cleanup/post-review.md`: 5 applied, 16 follow-ups |
| Docs | `docs/architecture/overview.md` (rewritten: the old module list named files that no longer exist), `platform-modularity.md` with a worked example |

## Decisions

* **Split only along a seam that can be checked.** Test modules, pure functions and types moved; the single-state-machine files
  (`TouchCapture.kt`, `AddDeviceWizard.tsx`, `adb_monitor`) were not cut, because the only check that would show a mistake is a
  tablet or a device in hand.
* **A dispatcher only where three or more backends already exist.** Display has five; input injection, the capture source and the
  service manager each have one, so they got a documented seam and no trait.
* **KDE stays the fall-through** of `DisplayBackend`, as it was in the ladders, so an unknown desktop behaves as before.
* **Entry points keep their package** (`MainActivity`, the receivers): the manifest, the ADB intents and the device-admin XML name them.

## Problems met on the way

* A tagging script that re-ran inserted duplicate `PLATFORM` lines above long doc blocks (its duplicate test looked eight lines
  up); found while reading a diff, removed in its own commit.
* The first slice that moved the KDE disable code ended at the wrong brace and deleted the end of an `impl`; the compiler caught
  it, the file was restored from git and the move redone.
* The settings-parity script read `PowerSettings.tsx`; after the split it reported 32 differences until it was pointed at
  `power/shared.ts`. Its desktop key count went from 35 to 33 because the old regex also counted two props of `ModeSection`.
* Fully qualified `com.uscreen.TouchpadShape` in a default argument was not caught by the import rewrite; the Kotlin compiler was.
* `cargo +nightly udeps` was run once to look for unused crates; `cargo-udeps` is not installed, and rustup synced the nightly
  toolchain as a side effect (`rustup toolchain list` shows a nightly now; whether it existed before was not checked).
  `cargo machete` was used instead.

## Result

Cargo tests 217 + 11, Android tests 44 → 47, installer tests 60, parity 0 differences, clippy, fmt, `tsc` and `vite build` clean,
`./build-all.sh` succeeds, installed binaries equal `dist/`, and the daemon updated the tablet's app to the new build and mapped
pen and touch on GNOME. Not verified: touch, pen and touchpad behaviour of the tablet app after the package move; Hyprland, COSMIC
and KDE paths; the desktop app's screens. Details in `docs/cleanup/CHANGES.md`.
