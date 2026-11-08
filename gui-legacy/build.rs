use std::process::Command;

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
                println!(
                    "cargo:warning=WARNING: Building with uncommitted changes. Marking as dirty."
                );
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
}
