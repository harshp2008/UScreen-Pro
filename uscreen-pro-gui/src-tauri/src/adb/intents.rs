//! Asking a device to do something visible: open its developer options or Wireless debugging screen, or announce itself
//! with a notification (Identify). Owns the intents and notification text; installing and listing are elsewhere.

use super::*;

/// `am start` exits 0 even when the activity is missing, so inspect its output.
async fn start_intent(serial: Option<&str>, action: &str) -> Result<(), String> {
    let out = run(
        serial,
        &["shell", "am", "start", "-a", action],
        Duration::from_secs(8),
    )
    .await?;
    let text = combined_of(&out);
    if !out.status.success() || text.contains("Error") || text.contains("error:") {
        Err(text)
    } else {
        Ok(())
    }
}

/// Tauri command: open the Developer Options screen on the tablet so the user can enable USB/Wireless debugging.
#[tauri::command]
pub async fn open_developer_options(serial: String) -> Result<String, String> {
    require_serial(&serial)?;
    start_intent(
        Some(&serial),
        "android.settings.APPLICATION_DEVELOPMENT_SETTINGS",
    )
    .await
    .map(|_| "Opened Developer Options on the device".to_string())
}

/// Tauri command: open Wireless Debugging on the tablet, falling back to Developer Options on builds that reject the intent.
#[tauri::command]
pub async fn open_wireless_debugging_settings(serial: String) -> Result<String, String> {
    require_serial(&serial)?;
    // The Wireless Debugging page has no public action on stock Android, so some builds reject
    // this intent; Developer Options is the fallback.
    if start_intent(
        Some(&serial),
        "android.settings.WIRELESS_DEBUGGING_SETTINGS",
    )
    .await
    .is_ok()
    {
        return Ok("Opened Wireless Debugging".into());
    }
    start_intent(
        Some(&serial),
        "android.settings.APPLICATION_DEVELOPMENT_SETTINGS",
    )
    .await
    .map(|_| "Opened Developer Options; choose Wireless debugging".to_string())
}

/// Quote one argument for the device's shell. `adb shell a b c` joins its arguments with spaces and
/// the device shell splits them again, so anything with a space (a title, a nickname) needs quotes.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Title shown on the identify notification: `UScreen: <device name>`.
fn identify_title(name: &str) -> String {
    format!("UScreen: {name}")
}

/// Make one specific device announce itself so the user can tell which one the wizard is about to
/// set up: a heads-up notification titled with the device name plus a short buzz, all pinned to
/// `serial` with `-s`.
///
/// `cmd notification post` has no priority flag (`-p` does not exist; its notifications land in a
/// low-importance shell channel and never pop up), so the heads-up comes from the UScreen app itself:
/// its `IdentifyReceiver` posts on an IMPORTANCE_HIGH channel. If the app is not installed yet (the
/// wizard identifies before it installs), the shell notification with the same title is the fallback.
/// `name` is the device nickname or model; without one the device's own `ro.product.model` is used.
#[tauri::command]
pub async fn identify_device(serial: String, name: Option<String>) -> Result<String, String> {
    require_serial(&serial)?;
    let mut name: String = name
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if name.is_empty() {
        name = run(
            Some(&serial),
            &["shell", "getprop", "ro.product.model"],
            Duration::from_secs(4),
        )
        .await
        .map(|o| stdout_of(&o))
        .unwrap_or_default();
    }
    if name.is_empty() {
        name = serial.clone();
    }
    let title = identify_title(&name);

    // 1. High-importance heads-up from the app (needs POST_NOTIFICATIONS on Android 13+, which the
    //    shell may grant).
    let _ = run(
        Some(&serial),
        &[
            "shell",
            "pm",
            "grant",
            "com.uscreen",
            "android.permission.POST_NOTIFICATIONS",
        ],
        Duration::from_secs(5),
    )
    .await;
    let mut notified = false;
    let mut text = String::new();
    if let Ok(out) = run(
        Some(&serial),
        &[
            "shell",
            "am",
            "broadcast",
            "-n",
            "com.uscreen/.IdentifyReceiver",
            "--es",
            "name",
            &sh_quote(&name),
        ],
        Duration::from_secs(8),
    )
    .await
    {
        text = combined_of(&out);
        notified = text.contains("data=\"posted\"");
    }

    // 2. Fallback: plain shell notification (title still carries the name), no heads-up.
    if !notified {
        let out = run(
            Some(&serial),
            &[
                "shell",
                "cmd",
                "notification",
                "post",
                "-S",
                "bigtext",
                "-t",
                &sh_quote(&title),
                "uscreen-identify",
                &sh_quote("This is your target device!"),
            ],
            Duration::from_secs(8),
        )
        .await?;
        let t = combined_of(&out);
        notified = out.status.success()
            && !t.to_lowercase().contains("error")
            && !t.contains("Unknown command");
        text = t;
    }

    let buzzed = run(
        Some(&serial),
        &[
            "shell",
            "cmd",
            "vibrator_manager",
            "synced",
            "-f",
            "oneshot",
            "500",
            "255",
        ],
        Duration::from_secs(5),
    )
    .await
    .map(|o| o.status.success() && combined_of(&o).is_empty())
    .unwrap_or(false);
    match (notified, buzzed) {
        (true, true) => Ok(format!(
            "\"{title}\" notification sent and the device vibrated"
        )),
        (true, false) => Ok(format!("\"{title}\" notification sent to the device")),
        (false, true) => Ok("The device vibrated".into()),
        (false, false) => Err(format!("The device did not react: {text}")),
    }
}

/// Tests of the identify text and of the serial check in the commands of this module.
#[cfg(test)]
mod tests {
    use super::*;

    /// The identify title and shell quoting handle apostrophes.
    #[test]
    fn identify_title_and_quoting() {
        assert_eq!(identify_title("Tab S9"), "UScreen: Tab S9");
        assert_eq!(sh_quote("Dipal's tab"), "'Dipal'\\''s tab'");
    }

    /// Commands that take a serial return an error instead of running against the default device.
    #[tokio::test]
    async fn serial_commands_refuse_to_run_without_a_target() {
        assert!(identify_device(String::new(), None).await.is_err());
        assert!(open_developer_options(String::new()).await.is_err());
        assert!(open_wireless_debugging_settings("  ".into()).await.is_err());
    }
}
