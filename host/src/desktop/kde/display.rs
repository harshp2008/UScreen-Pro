//! Talking to KWin over D-Bus.
//!
//! Every call here used to shell out to `qdbus`, which turned out not to be a
//! safe assumption. On Debian- and Ubuntu-based Plasma 6 systems the binary is
//! `qdbus-qt6` or `qdbus6`, and plain `qdbus` — Qt5's — is often not installed
//! at all. When it was missing the daemon stopped mapping the tablet's input
//! onto the virtual display and said so only in a log line nobody reads, so
//! touches drove whatever screen the cursor happened to be on. That is what
//! issues #9 and #10 both were.
//!
//! `busctl` ships with systemd, so it is present on every distribution this
//! project targets, and it is tried first; the qdbus family stays as a
//! fallback for systems without it.
// PLATFORM(kde): virtual display on KDE Plasma: kscreen-doctor for outputs, KWin D-Bus (busctl) for input device properties

use tokio::sync::OnceCell;
use tracing::{debug, info, warn};

/// D-Bus service name of KWin.
const SERVICE: &str = "org.kde.KWin";
/// Probe target: a property KWin always exposes, so a tool that exists but
/// cannot reach the session bus is rejected here instead of failing on every
/// call afterwards.
const PROBE_PATH: &str = "/VirtualKeyboard";
/// D-Bus interface of the probe property.
const PROBE_IFACE: &str = "org.kde.kwin.VirtualKeyboard";
/// Name of the probe property, `available`.
const PROBE_PROP: &str = "available";

/// Names under which a qdbus tool may be installed.
const QDBUS_NAMES: [&str; 4] = ["qdbus", "qdbus6", "qdbus-qt6", "qdbus-qt5"];

/// Tool used to talk to KWin over D-Bus: `busctl`, or one of the qdbus variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Busctl,
    Qdbus(&'static str),
}

impl Backend {
    /// Command name of the backend, for logs.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Busctl => "busctl",
            Backend::Qdbus(cmd) => cmd,
        }
    }
}

/// The detected D-Bus tool, probed once.
static BACKEND: OnceCell<Option<Backend>> = OnceCell::const_new();

/// Run `cmd`; its stdout, or `None` if it could not be run.
async fn output_of(cmd: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Does an `XDG_CURRENT_DESKTOP` value denote KDE? An empty value counts as KDE so a bare session is still tried.
pub fn is_kde_session(desktop: &str) -> bool {
    if desktop.is_empty() {
        return true;
    }
    desktop
        .split(':')
        .any(|entry| entry.eq_ignore_ascii_case("KDE"))
}

/// [`is_kde_session`] applied to the current environment.
pub fn in_kde_session() -> bool {
    is_kde_session(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())
}

/// Which tool can reach KWin on this machine, decided once per run.
pub async fn backend() -> Option<Backend> {
    *BACKEND
        .get_or_init(|| async {
            if !in_kde_session() {
                debug!("Not a KDE Plasma session — KWin is not looked for");
                return None;
            }
            if output_of(
                "busctl",
                &[
                    "--user",
                    "get-property",
                    SERVICE,
                    PROBE_PATH,
                    PROBE_IFACE,
                    PROBE_PROP,
                ],
            )
            .await
            .is_some()
            {
                info!("KWin D-Bus via busctl");
                return Some(Backend::Busctl);
            }
            for cmd in QDBUS_NAMES {
                if output_of(
                    cmd,
                    &[
                        "--literal",
                        SERVICE,
                        PROBE_PATH,
                        "org.freedesktop.DBus.Properties.Get",
                        PROBE_IFACE,
                        PROBE_PROP,
                    ],
                )
                .await
                .is_some()
                {
                    info!("KWin D-Bus via {}", cmd);
                    return Some(Backend::Qdbus(cmd));
                }
            }
            warn!(
                "Cannot reach KWin over D-Bus: neither busctl nor qdbus answered. \
                 Touch and pen will address the whole desktop instead of the tablet's \
                 screen, and the on-screen keyboard will not be suppressed. \
                 On KDE this normally means systemd's busctl is missing."
            );
            None
        })
        .await
}

/// A scalar property. Strings come back unquoted, numbers as digits.
pub async fn get_property(path: &str, iface: &str, prop: &str) -> Option<String> {
    match backend().await? {
        Backend::Busctl => {
            // `s "DVI-I-1"`, `i 1`, `b true` — signature first, then the value.
            let raw = output_of(
                "busctl",
                &["--user", "get-property", SERVICE, path, iface, prop],
            )
            .await?;
            let value = raw.split_once(' ').map(|(_, v)| v).unwrap_or(&raw).trim();
            Some(value.trim_matches('"').to_string())
        }
        Backend::Qdbus(cmd) => {
            // `[Variant(QString): "DVI-I-1"]`, `[Variant(int): 1]`
            let raw = output_of(
                cmd,
                &[
                    "--literal",
                    SERVICE,
                    path,
                    "org.freedesktop.DBus.Properties.Get",
                    iface,
                    prop,
                ],
            )
            .await?;
            if raw.contains('"') {
                let start = raw.find('"')? + 1;
                let end = raw.rfind('"')?;
                (end > start).then(|| raw[start..end].to_string())
            } else {
                let value = raw.rsplit_once(':').map(|(_, v)| v).unwrap_or(&raw);
                Some(
                    value
                        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
                        .to_string(),
                )
            }
        }
    }
}

/// Write a scalar property. `signature` is the D-Bus type: `s` for a string,
/// `i` for an int — busctl needs it, qdbus infers it.
pub async fn set_property(
    path: &str,
    iface: &str,
    prop: &str,
    signature: &str,
    value: &str,
) -> bool {
    match backend().await {
        Some(Backend::Busctl) => output_of(
            "busctl",
            &[
                "--user",
                "set-property",
                SERVICE,
                path,
                iface,
                prop,
                signature,
                value,
            ],
        )
        .await
        .is_some(),
        Some(Backend::Qdbus(cmd)) => output_of(
            cmd,
            &[
                "--literal",
                SERVICE,
                path,
                "org.freedesktop.DBus.Properties.Set",
                iface,
                prop,
                value,
            ],
        )
        .await
        .is_some(),
        None => false,
    }
}

/// An array-of-strings property, such as the list of input devices.
pub async fn list_strings(path: &str, iface: &str, prop: &str) -> Option<Vec<String>> {
    match backend().await? {
        Backend::Busctl => {
            // `as 12 "event7" "event8" …` — every other field between quotes.
            let raw = output_of(
                "busctl",
                &["--user", "get-property", SERVICE, path, iface, prop],
            )
            .await?;
            Some(
                raw.split('"')
                    .skip(1)
                    .step_by(2)
                    .map(str::to_string)
                    .collect(),
            )
        }
        Backend::Qdbus(cmd) => {
            // qdbus reads a property through its member name, one per line.
            let member = format!("{}.{}", iface, prop);
            let raw = output_of(cmd, &[SERVICE, path, &member]).await?;
            Some(raw.split_whitespace().map(str::to_string).collect())
        }
    }
}

/// Tests for parsing `busctl` output and for KDE session detection.
#[cfg(test)]
mod tests {
    use super::*;

    /// A `busctl` string array is split on its quotes.
    #[test]
    fn busctl_string_array_is_split_on_quotes() {
        let raw = r#"as 3 "event7" "event8" "event4""#;
        let got: Vec<String> = raw
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect();
        assert_eq!(got, vec!["event7", "event8", "event4"]);
    }

    /// A `busctl` scalar reply loses its type signature and quotes.
    #[test]
    fn busctl_scalar_drops_the_signature_and_quotes() {
        for (raw, want) in [(r#"s "DVI-I-1""#, "DVI-I-1"), ("i 1", "1"), (r#"s """#, "")] {
            let value = raw.split_once(' ').map(|(_, v)| v).unwrap_or(raw).trim();
            assert_eq!(value.trim_matches('"'), want);
        }
    }

    /// `is_kde_session` finds KDE in a colon-separated desktop list ignoring case, and treats an empty value as KDE.
    #[test]
    fn is_kde_session_reads_the_desktop_list() {
        assert!(is_kde_session("KDE"));
        assert!(is_kde_session("kde"));
        assert!(is_kde_session("GNOME:KDE"));
        assert!(is_kde_session(""));
        assert!(!is_kde_session("ubuntu:GNOME"));
        assert!(!is_kde_session("X-Cinnamon"));
        assert!(!is_kde_session("COSMIC"));
        assert!(!is_kde_session("Hyprland"));
    }
}
