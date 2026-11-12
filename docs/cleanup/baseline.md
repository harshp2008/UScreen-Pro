<!--
  docs/cleanup/baseline.md: results of every check before the chore/codebase-cleanup branch changed anything.
  Owns: the "before" column. The "after" column is in docs/cleanup/CHANGES.md.
-->

# Baseline (branch `feat/settings-ui-overhaul`, commit `ed97427`)

| Check | Result before the cleanup |
|---|---|
| `cargo test --workspace` | pass: 217 passed, 2 ignored (host); 11 passed (Tauri crate); 0 in `menu` |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo fmt --check` | **fail**: 5 hunks in `uscreen-pro-gui/src-tauri/build.rs` and `src/adb/install.rs`. Fixed in the first commit of the branch (formatting only) |
| GUI typecheck: `npx tsc --noEmit` | pass |
| GUI lint and tests | none exist: `package.json` has only `dev`, `build`, `preview`, `tauri` |
| Android `./gradlew testDebugUnitTest --offline` | pass |
| `python3 scripts/tests/settings_parity.py` | pass: host 36 keys, tablet 39, desktop 35, 0 differences |

Output was written to files and read in full, not piped through `tail`.
