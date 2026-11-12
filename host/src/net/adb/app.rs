//! The Android app on a connected tablet: whether it runs, launching it with the session's ports and token, and the
//! work done when a tablet first connects. Owns app supervision; discovery and tunnels are elsewhere.

use super::*;

/// Determine whether the UScreen client process (`com.uscreen`) is currently running on the device.
pub async fn app_running(serial: &str) -> Option<bool> {
    let out = Command::new("adb")
        .args(["-s", serial, "shell", "pidof", "com.uscreen"])
        .output()
        .await
        .ok()?;
    if !out.stderr.is_empty() {
        return None;
    }
    Some(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// What the tablet is showing, as far as opening the app by itself is concerned.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ScreenState {
    /// The lock screen is up.
    pub keyguard: bool,
    /// The screen is off or dozing.
    pub asleep: bool,
    /// Package that has the input focus, if any.
    pub focus: Option<String>,
    /// Package of the home screen (launcher).
    pub launcher: Option<String>,
}

impl ScreenState {
    /// Parse the output of the one `adb shell` command in [`screen_state`]: `dumpsys window`'s
    /// `mCurrentFocus` and `isKeyguardShowing` lines, `cmd package resolve-activity`'s
    /// `packageName=` line for the home activity and `dumpsys power`'s `mWakefulness=` line.
    pub(super) fn parse(out: &str) -> Self {
        let mut s = ScreenState {
            keyguard: false,
            asleep: false,
            focus: None,
            launcher: None,
        };
        for line in out.lines() {
            let line = line.trim();
            if line.contains("isKeyguardShowing=true") {
                s.keyguard = true;
            } else if let Some(rest) = line.strip_prefix("mCurrentFocus=") {
                // `Window{4f60b6d u0 com.uscreen/com.uscreen.MainActivity}`
                s.focus = rest
                    .split_whitespace()
                    .find_map(|w| w.split('/').next().filter(|_| w.contains('/')))
                    .map(|p| p.trim_start_matches('{').to_string());
            } else if let Some(pkg) = line.strip_prefix("packageName=") {
                s.launcher.get_or_insert_with(|| pkg.to_string());
            } else if let Some(w) = line.strip_prefix("mWakefulness=") {
                s.asleep = matches!(w, "Asleep" | "Dozing");
            }
        }
        s
    }

    /// Whether nothing the user is doing would be interrupted: the lock screen, an off screen,
    /// the home screen, or the UScreen app itself.
    pub(super) fn is_idle(&self) -> bool {
        if self.keyguard || self.asleep {
            return true;
        }
        match (&self.focus, &self.launcher) {
            (Some(f), Some(l)) if f == l => true,
            (Some(f), _) => f == "com.uscreen",
            _ => false,
        }
    }
}

// PLATFORM(android): parses dumpsys output and a HOME-intent query to tell whether the tablet shows its launcher or lock screen
/// Ask the tablet what it is showing; `None` if adb could not tell.
async fn screen_state(serial: &str) -> Option<ScreenState> {
    let script = "dumpsys window | grep -E 'mCurrentFocus|isKeyguardShowing'; \
        cmd package resolve-activity -a android.intent.action.MAIN -c android.intent.category.HOME | grep packageName= | head -1; \
        dumpsys power | grep -E '^ *mWakefulness='";
    let out = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new("adb")
            .args(["-s", serial, "shell", script])
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    (!text.trim().is_empty()).then(|| ScreenState::parse(&text))
}

/// Devices whose app was running when an update replaced it, and when: the update closes the app, so it is opened again once, whatever
/// "Open app on connect" says. The mark expires after a minute.
fn reopen_marks() -> &'static std::sync::Mutex<HashMap<String, std::time::Instant>> {
    /// The marks, created on first use.
    static MARKS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> =
        std::sync::OnceLock::new();
    MARKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Woken when an update has finished for a device marked for reopening, so the monitor opens the app at once instead of at its next check.
pub fn update_done() -> &'static tokio::sync::Notify {
    /// The notification, created on first use.
    static DONE: std::sync::OnceLock<tokio::sync::Notify> = std::sync::OnceLock::new();
    DONE.get_or_init(tokio::sync::Notify::new)
}

/// Remember that the app of `serial` was running when it was updated and should be opened again, and say so.
pub fn reopen_after_update(serial: &str) {
    if let Ok(mut m) = reopen_marks().lock() {
        m.insert(serial.to_string(), std::time::Instant::now());
    }
    update_done().notify_one();
}

/// Whether `serial` is marked for reopening (and not stale).
pub fn marked_for_reopen(serial: &str) -> bool {
    reopen_marks()
        .lock()
        .ok()
        .and_then(|m| m.get(serial).copied())
        .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
}

/// [`marked_for_reopen`] for the tests of the parent module.
#[cfg(test)]
pub(super) fn marked_for_reopen_for_test(serial: &str) -> bool {
    marked_for_reopen(serial)
}

/// Whether the daemon may open the app on this tablet now, by the `auto_open_app` setting (read
/// fresh, so a change applies at once): never, only from the home or lock screen, or always.
pub async fn may_open_app(serial: &str) -> bool {
    if marked_for_reopen(serial) {
        return true;
    }
    match config::FileConfig::load().auto_open_app.as_str() {
        "always" => true,
        "idle" => screen_state(serial).await.is_some_and(|s| s.is_idle()),
        _ => false,
    }
}

/// Emit diagnostic telemetry when a session is active over a Wi-Fi transport.
pub fn announce_transport(serial: &str) {
    if transport_of(serial) == Transport::Network {
        warn!("Running over Wi-Fi. Expect occasional stutter — the cable is much steadier.");
    }
}

/// Invoked when a tablet is attached or transitions to ready state.
///
/// Sets up port forwarding and optionally launches the client APK with intent parameters.
pub async fn on_tablet_connected(
    serial: &str,
    video_port: u16,
    input_port: u16,
    token: Option<&str>,
) {
    match setup_adb_forwarding(serial, video_port, input_port).await {
        Ok(_) => {
            info!(
                "ADB port forwarding set up (video={}, input={})",
                video_port, input_port
            );
            if may_open_app(serial).await {
                launch_app(serial, token, video_port, input_port).await;
            }
        }
        Err(e) => warn!("ADB forwarding failed: {}", e),
    }
}

/// Launch or front the UScreen Android client application, passing the security token and port parameters.
pub async fn launch_app(serial: &str, token: Option<&str>, video_port: u16, input_port: u16) {
    if let Ok(mut m) = reopen_marks().lock() {
        m.remove(serial);
    }
    let mut cmd = format!(
        "am start -n com.uscreen/.MainActivity --ei video_port {} --ei input_port {}",
        video_port, input_port
    );
    if let Some(t) = token {
        cmd.push_str(" --es token ");
        cmd.push_str(t);
    }
    cmd.push_str(" >/dev/null 2>&1; exit\n");

    let child = Command::new("adb")
        .args(["-s", serial, "shell"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            warn!("Could not run adb: {}", e);
            return;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(cmd.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    match tokio::time::timeout(Duration::from_secs(15), child.wait()).await {
        Ok(Ok(st)) if st.success() => info!("UScreen app launched on device"),
        _ => warn!("Could not launch the app (is it installed?)"),
    }
}
