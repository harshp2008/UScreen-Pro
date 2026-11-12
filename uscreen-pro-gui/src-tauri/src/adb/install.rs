//! Installing the bundled APK on a device (with progress events) and launching the app afterwards. Owns the APK lookup
//! and the install sequence; device listing and settings intents are elsewhere.

use super::*;

/// The APK compiled into this app (empty when built without one, see `build.rs`).
static EMBEDDED_APK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.apk"));

// PLATFORM(linux): copies the embedded APK to ~/.local/share/uscreen/app-debug.apk
/// The APK to install: the one carried inside this app, written to `~/.local/share/uscreen/app-debug.apk`
/// (only when that file differs, so the background daemon's update path uses the same build); an app built
/// without an embedded APK falls back to searching the usual locations.
fn apk_to_install() -> Option<PathBuf> {
    if EMBEDDED_APK.is_empty() {
        return find_apk_path();
    }
    let path = config::home_dir().join(".local/share/uscreen/app-debug.apk");
    if std::fs::read(&path)
        .map(|b| b != EMBEDDED_APK)
        .unwrap_or(true)
    {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, EMBEDDED_APK).ok()?;
    }
    Some(path)
}

/// Candidate locations of the built APK; the most recently modified one wins so a stale
/// copy in `~/.local/share/uscreen` can never shadow a fresh build.
fn find_apk_path() -> Option<PathBuf> {
    let home = config::home_dir();
    let mut candidates = vec![
        home.join(".local/share/uscreen/app-debug.apk"),
        PathBuf::from("/usr/share/uscreen/app-debug.apk"),
        PathBuf::from("/usr/local/share/uscreen/app-debug.apk"),
    ];
    if let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) {
        for dir in exe.ancestors().skip(1).take(5) {
            candidates.push(dir.join("dist/app-debug.apk"));
            candidates.push(dir.join("android/app/build/outputs/apk/debug/app-debug.apk"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors().take(4) {
            candidates.push(dir.join("dist/app-debug.apk"));
        }
    }
    candidates
        .into_iter()
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((modified, p))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

/// Install the bundled APK on exactly `serial`. With `replace_existing` an already installed copy is
/// removed first (the wizard does this so an APK signed with another key, e.g. a release of the
/// original project, cannot block the install); the queue/update path keeps `-r` so app data survives.
pub async fn install_apk(
    app: Option<&AppHandle>,
    serial: &str,
    replace_existing: bool,
) -> Result<String, String> {
    require_serial(serial)?;
    let apk = apk_to_install()
        .ok_or("Could not find app-debug.apk. Run ./build-all.sh first.".to_string())?;
    let apk = apk.to_string_lossy().into_owned();
    let ev = "install_progress";

    if replace_existing {
        let installed = run(
            Some(serial),
            &["shell", "pm", "path", "com.uscreen"],
            Duration::from_secs(8),
        )
        .await
        .map(|o| stdout_of(&o).contains("package:"))
        .unwrap_or(false);
        if installed {
            progress(app, ev, Some(serial), "Found old APK");
            // Ask the installed app to drop its Device Admin role first (an active admin cannot be
            // uninstalled). Builds without the receiver just ignore this; the retry below covers them.
            progress(
                app,
                ev,
                Some(serial),
                "Asking the old app to remove its Device Admin",
            );
            let _ = run(
                Some(serial),
                &[
                    "shell",
                    "am",
                    "broadcast",
                    "-W",
                    "-n",
                    "com.uscreen/.RemoveAdminReceiver",
                ],
                Duration::from_secs(10),
            )
            .await;
            progress(app, ev, Some(serial), "Removing old APK");
            let mut out = run(
                Some(serial),
                &["uninstall", "com.uscreen"],
                Duration::from_secs(30),
            )
            .await?;
            let mut text = combined_of(&out);
            if !text.contains("Success") && text.contains("DEVICE_POLICY_MANAGER") {
                // The old app is an active device admin (it uses force-lock), which Android will not
                // uninstall. Try to deactivate the admin and retry; if that is refused too, fall through
                // and update in place with `install -r` (works when the signing key matches).
                progress(
                    app,
                    ev,
                    Some(serial),
                    "Old APK is a device admin, deactivating it",
                );
                let _ = run(
                    Some(serial),
                    &[
                        "shell",
                        "dpm",
                        "remove-active-admin",
                        "com.uscreen/.UScreenAdminReceiver",
                    ],
                    Duration::from_secs(10),
                )
                .await;
                out = run(
                    Some(serial),
                    &["uninstall", "com.uscreen"],
                    Duration::from_secs(30),
                )
                .await?;
                text = combined_of(&out);
                if !text.contains("Success") {
                    progress(
                        app,
                        ev,
                        Some(serial),
                        "Could not remove it, updating the installed app in place",
                    );
                }
            } else if !text.contains("Success") {
                return Err(format!("Could not remove the old APK: {text}"));
            }
        }
    }
    progress(app, ev, Some(serial), "Installing new APK");
    let out = run(
        Some(serial),
        &["install", "-r", "-d", &apk],
        Duration::from_secs(120),
    )
    .await?;
    let text = combined_of(&out);
    if text.contains("Success") {
        progress(app, ev, Some(serial), "APK installed");
        // Open the app right away so the device sits on "waiting for host" without a tap. Best effort:
        // a locked screen or a launcher that blocks background starts must not fail a good install.
        progress(app, ev, Some(serial), "Launching the app");
        if let Err(e) = launch_app(serial).await {
            progress(
                app,
                ev,
                Some(serial),
                &format!("App installed but could not be opened automatically ({e})"),
            );
        }
        Ok("APK installed".into())
    } else {
        Err(format!("Install failed: {text}"))
    }
}

/// Start the UScreen activity on exactly `serial`: `am start -n com.uscreen/.MainActivity`.
/// `am start` exits 0 even when the activity is missing, so its output is checked too.
pub async fn launch_app(serial: &str) -> Result<(), String> {
    require_serial(serial)?;
    let out = run(
        Some(serial),
        &["shell", "am", "start", "-n", "com.uscreen/.MainActivity"],
        Duration::from_secs(8),
    )
    .await?;
    let text = combined_of(&out);
    if !out.status.success() || text.contains("Error") || text.contains("Exception") {
        Err(text)
    } else {
        Ok(())
    }
}

/// Tauri command: install the bundled APK on one device, emitting progress events to the UI.
#[tauri::command]
pub async fn install_apk_on_device(app: AppHandle, serial: String) -> Result<String, String> {
    install_apk(Some(&app), &serial, true).await
}
