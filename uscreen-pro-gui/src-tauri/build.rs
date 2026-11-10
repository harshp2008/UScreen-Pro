//! Build script of the desktop app: stamps `GIT_VERSION_STRING`, then runs the Tauri build step.

use std::process::Command;

/// Sets `GIT_VERSION_STRING` for the compiler from the environment or the git hash, as the host's build script does.
fn main() {
    let version_string = std::env::var("GIT_VERSION_STRING")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let git_hash = Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|out| {
                    if out.status.success() {
                        String::from_utf8(out.stdout)
                            .ok()
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

            let is_dirty = Command::new("git")
                .args(["status", "--porcelain"])
                .output()
                .ok()
                .map(|out| {
                    if out.status.success() {
                        !out.stdout.is_empty()
                    } else {
                        false
                    }
                })
                .unwrap_or(false);

            if is_dirty {
                format!("{}-dirty", git_hash)
            } else {
                git_hash
            }
        });

    println!("cargo:rustc-env=GIT_VERSION_STRING={}", version_string);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-env-changed=GIT_VERSION_STRING");

    embed_apk();
    tauri_build::build();
}

/// Copies the Android APK into `OUT_DIR/app.apk` so the app can carry it (`include_bytes!` in `adb/install.rs`).
/// The APK is `$USCREEN_APK` or `dist/app-debug.apk` at the repo root (built first by `build-all.sh`); when it is
/// missing (a plain `cargo check`, CI) an empty file is written and the app falls back to searching for an APK.
fn embed_apk() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("app.apk");
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = std::env::var("USCREEN_APK")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../../dist/app-debug.apk"));
    println!("cargo:rerun-if-env-changed=USCREEN_APK");
    println!("cargo:rerun-if-changed={}", src.display());
    if std::fs::copy(&src, &out).is_err() {
        println!("cargo:warning=no APK at {}; the app will not embed one", src.display());
        std::fs::write(&out, []).expect("write placeholder APK");
    }
}
