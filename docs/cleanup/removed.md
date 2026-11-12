<!--
  docs/cleanup/removed.md: everything the chore/codebase-cleanup branch deleted or merged, with the evidence that nothing used it,
  plus the items that look unused but were kept. Owns that list. Earlier removals are in docs/codebase-changes.md.
-->

# Removed, merged, and suspicious-but-kept

## How unused code was looked for

| Tool | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` (dead code, unused imports and parameters) | clean at baseline |
| `cargo machete` (unused crates) | "didn't find any unused dependencies" |
| `npx tsc --noEmit --noUnusedLocals --noUnusedParameters` | clean |
| `./gradlew compileDebugKotlin --rerun-tasks` | only three deprecation warnings (`setDecorFitsSystemWindows`, `ACTION_UNINSTALL_PACKAGE`, `rememberTooltipPositionProvider`); no unused-symbol warnings |
| a script counting how often every declared name (Rust, Kotlin, TS, shell, Python, JS) appears in the repository | left only test names, framework overrides (`onOpen`, `onBind`, ...) and the installer's `<family>_*` functions that `scripts/lib/platform.sh` calls by name, plus the one item removed below |
| exported TypeScript names used once; CSS classes in `index.css` never named in a source file | none |
| `grep` for commented-out code, `TODO`, `FIXME`, `HACK` | none (the matches were prose or `.toDouble()`) |
| `cargo +nightly udeps` | not available (`cargo-udeps` is not installed; `cargo machete` was used instead) |

## Removed or merged

| Item | File | Evidence that nothing used it |
|---|---|---|
| `#[allow(unused_labels)]` on the `'session` loop | `host/src/capture/supervisor.rs` | the label is used by `continue 'session` further down the same loop (`supervisor.rs`); clippy with `-D warnings` is clean without the attribute |
| `val SurfaceHigh` (a `@Composable` colour getter) | `android/.../ui/Theme.kt` | the name appears once in the whole repository, at its declaration. Compose has no reflection lookup of it. The palette field `surfaceHigh` it read is still used by both colour schemes |
| three copies of `errText` | `App.tsx`, `ManualPairDialog.tsx`, `AddDeviceWizard.tsx` | identical one-line helpers; now one exported `errText` in `lib/utils.ts`. `tsc` is clean |

## Suspicious but kept

| Item | Why it was kept |
|---|---|
| Cargo feature `inproc-encoder` (`host/Cargo.toml`, `capture/encoder.rs`, `#[cfg]` blocks in `capture/`) | never built by `build-all.sh`, the Makefile or CI, so it can rot, but it is documented in `docs/development.md` and benchmarked in `docs/benchmarks.md`. Removing it is a product decision. It needs the ffmpeg development headers, which are not installed here, so its `cfg` blocks could not be compiled |
| `#[cfg_attr(not(feature = "inproc-encoder"), allow(dead_code))]` in `capture/mod.rs` | belongs to the same feature |
| `Makefile` `VERSION = 1.2.12` while the crates say `1.3.0` | only the release-metadata and tarball targets read it; changing it changes release artefacts |
| `host/src/daemon/update.rs` queries `majmichu1/UScreen` | behaviour: it decides which releases the daemon announces. Already recorded in `docs/codebase-changes.md` |
| the host's handling of the old `mode` message | protocol compatibility with older tablet apps |
| `allow_legacy_fallback` and the legacy-port code in `host/src/net/pairing.rs` | a config key (default off) that is mirrored in the GUI and checked by `settings_parity.py` |
| `uscreen-pro-gui/src/lib/devMock.ts` | loaded only by `vite dev` in a plain browser (`main.tsx`); invisible to the Tauri build but used for UI work |
| the installer's `<family>_*` shell functions and the `t_*` test functions in `scripts/tests/run.sh` | reached by name (`"${family}_order"`, a loop over `t_*`), so a text search finds one occurrence |
| `FadeCurve.getInterpolation`, `onOrientationChanged`, `onBind`, `surfaceChanged`, `surfaceDestroyed` | overrides called by Android |
| `scripts/redact.sh`, `scripts/rca56_guarded_test.sh`, `scripts/enc_latency_probe.py`, `scripts/e2e_input_engine.sh` | referenced from experiment records or used by hand by the maintainer |
| the test `physical_device_deduplication_prefers_usb` in `host/src/net/adb/mod.rs` | re-implements the merge inline, so it tests itself; a real test needs a fake `adb` |
