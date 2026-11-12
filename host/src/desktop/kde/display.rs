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
/// D-Bus interface name of a KWin input device.
pub const KWIN_INPUT_IFACE: &str = "org.kde.KWin.InputDevice";

// PLATFORM(kde): finds the laptop panel with kscreen-doctor
/// The screen the user is actually looking at: the first enabled output that is
/// not one of ours. In pen-only mode the tablet drives this one, so the pen has
/// to be mapped onto it rather than onto the virtual display.
pub async fn primary_non_evdi_output() -> Option<String> {
    let evdi: Vec<String> = crate::display::vdisplay::evdi_connectors()
        .into_iter()
        .map(|c| c.name)
        .collect();
    let out = tokio::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .await
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let outputs = v.get("outputs")?.as_array()?;
    let mut fallback = None;
    // Plasma 6 dropped the boolean `primary` for a `priority` where 1 is the
    // primary screen; looking only for `primary` found none and took the first
    // enabled output in the list, which on a multi-monitor desktop is not
    // necessarily the one in front of the user.
    let mut best: Option<(u64, String)> = None;
    for o in outputs {
        let name = o.get("name").and_then(|v| v.as_str())?.to_string();
        if evdi.contains(&name) || !o.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if o.get("primary").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Some(name);
        }
        if let Some(p) = o.get("priority").and_then(|v| v.as_u64()) {
            if best.as_ref().is_none_or(|(bp, _)| p < *bp) {
                best = Some((p, name.clone()));
            }
        }
        fallback.get_or_insert(name);
    }
    best.map(|(_, n)| n).or(fallback)
}

// PLATFORM(kde): reads a property of a KWin input device over D-Bus
/// Read one property of a KWin input device by its sysname.
pub async fn kwin_device_property(sysname: &str, property: &str) -> Option<String> {
    crate::desktop::kde::display::get_property(
        &format!("/org/kde/KWin/InputDevice/{}", sysname),
        KWIN_INPUT_IFACE,
        property,
    )
    .await
}

// PLATFORM(kde): output rectangle from kscreen-doctor JSON
/// Where `output` sits on the desktop, as fractions of the whole desktop:
/// (x, y, width, height). `None` when the layout cannot be read.
///
/// Used only for the plain pointer device — see `InjectDevices::pointer_area`.
pub async fn output_area(output: &str) -> Option<(f64, f64, f64, f64)> {
    let outputs = kscreen_outputs().await?;
    let logical = |o: &serde_json::Value| -> Option<(f64, f64, f64, f64)> {
        let scale = o
            .get("scale")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .max(0.01);
        let id = o.get("currentModeId")?.as_str()?;
        let mode = o
            .get("modes")?
            .as_array()?
            .iter()
            .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(id))?;
        let w = mode.pointer("/size/width")?.as_f64()? / scale;
        let h = mode.pointer("/size/height")?.as_f64()? / scale;
        let x = o.pointer("/pos/x")?.as_f64()?;
        let y = o.pointer("/pos/y")?.as_f64()?;
        Some((x, y, w, h))
    };
    let mut mine = None;
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for o in &outputs {
        if !o.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some((x, y, w, h)) = logical(o) else {
            continue;
        };
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
        if o.get("name").and_then(|v| v.as_str()) == Some(output) {
            mine = Some((x, y, w, h));
        }
    }
    let (x, y, w, h) = mine?;
    (max_x > 0.0 && max_y > 0.0).then(|| (x / max_x, y / max_y, w / max_x, h / max_y))
}

// PLATFORM(kde): runs kscreen-doctor -j and parses its JSON
/// The outputs KWin currently knows, as reported by `kscreen-doctor -j`.
/// `None` when the tool is not there at all (not a KDE session).
pub async fn kscreen_outputs() -> Option<Vec<serde_json::Value>> {
    let out = tokio::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .await
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(v.get("outputs")?.as_array()?.clone())
}

/// The output the devices should address in this mode, returned only once
/// KWin lists it as enabled.
///
/// Mapping is not something that can be done ahead of time: set `outputName`
/// while the virtual display is still being brought back and KWin keeps the
/// previous mapping, so after leaving graphics-tablet mode the pen and touch
/// would go on driving the laptop screen (issue #6). Leaving display mode has
/// the opposite problem — the physical screen is always there, but the EVDI
/// output is going away at that moment. So this polls the output list up to
/// `timeout` and only then falls back to the best name it knows, so a mapping
/// is at least attempted on a desktop where kscreen-doctor cannot answer.
pub async fn target_output(
    pen_only: bool,
    card: Option<u32>,
    timeout: std::time::Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if pen_only {
            if let Some(name) = primary_non_evdi_output().await {
                return Some(name);
            }
            // No kscreen-doctor means no KDE session: nothing to wait for.
            kscreen_outputs().await?;
        } else {
            let connectors = crate::display::vdisplay::evdi_connectors();
            // This tablet's own card first; "any connected EVDI output" only
            // as a fallback while the card is not known yet.
            let fallback = connectors
                .iter()
                .find(|c| card.is_some_and(|want| c.card == want))
                .or_else(|| connectors.iter().find(|c| c.connected))
                .or_else(|| connectors.first())
                .map(|c| c.name.clone());
            let Some(outputs) = kscreen_outputs().await else {
                return fallback;
            };
            let mine: Vec<&str> = connectors
                .iter()
                .filter(|c| card.is_none_or(|want| c.card == want))
                .map(|c| c.name.as_str())
                .collect();
            let enabled = outputs.iter().find(|o| {
                let name = o.get("name").and_then(|v| v.as_str()).unwrap_or("");
                mine.contains(&name) && o.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false)
            });
            if let Some(o) = enabled {
                return o.get("name").and_then(|v| v.as_str()).map(str::to_string);
            }
            if tokio::time::Instant::now() >= deadline {
                if fallback.is_some() {
                    warn!(
                        "EVDI output not enabled within {:?} — mapping onto {:?} anyway",
                        timeout, fallback
                    );
                }
                return fallback;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

// PLATFORM(kde): enable the EVDI output and place it beside the other screens with one atomic kscreen-doctor call
/// Switch the virtual output on and place it on the `position` side of the other screens, with `kscreen-doctor`.
///
/// `evdi_names` are the connector names of this tablet's EVDI card. KWin may not know the new device yet, so
/// `kscreen-doctor -j` is polled for up to 3 s (15 tries, 200 ms apart); when nothing shows up a warning is logged and the
/// output is left alone. Moves the other outputs too when the virtual one would land at a negative position.
pub async fn enable_output(evdi_names: &[String], position: crate::config::Position) {
    // Retry: KWin may not have registered the new EVDI device yet.
    // Use -j (JSON) rather than the plain "-o" text listing: newer
    // kscreen-doctor versions emit ANSI color codes in "-o" output
    // unconditionally, even when piped to a non-tty, which broke the
    // old line-based "Output:"/"Geometry:" parser (it silently matched
    // nothing, since every line actually starts with an escape code).
    for attempt in 0..15 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        let Ok(o) = tokio::process::Command::new("kscreen-doctor")
            .arg("-j")
            .output()
            .await
        else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) else {
            continue;
        };
        let Some(outputs) = v.get("outputs").and_then(|o| o.as_array()) else {
            continue;
        };

        // (id, enabled, x, y, logical size)
        let mut evdi: Option<(u32, bool, i64, i64, i64, i64)> = None;
        // Every other enabled output, so the whole layout can be shifted
        // back to the origin when the virtual screen lands above or left
        // of everything.
        let mut others: Vec<(u32, i64, i64)> = Vec::new();
        // Bounding box of everything already on the desktop. The virtual
        // screen is placed against one of its edges, so all four are
        // needed, not just the right one.
        let mut have_any = false;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (0i64, 0i64, 0i64, 0i64);

        for out in outputs {
            let id = out.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let name = out.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let enabled = out
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let x = out.pointer("/pos/x").and_then(|v| v.as_i64()).unwrap_or(0);

            let y = out.pointer("/pos/y").and_then(|v| v.as_i64()).unwrap_or(0);
            let raw_w = out
                .pointer("/size/width")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let raw_h = out
                .pointer("/size/height")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let sc = out.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let logical = |v: i64| {
                if sc > 0.0 {
                    (v as f64 / sc).round() as i64
                } else {
                    v
                }
            };

            if evdi_names.iter().any(|n| n == name) {
                evdi = Some((id, enabled, x, y, logical(raw_w), logical(raw_h)));
            } else if enabled {
                // `pos` is already in logical (scaled) coordinates,
                // matching what "position.X,Y" expects, but `size`
                // is the raw physical mode resolution — divide by
                // `scale` to get the logical width before adding,
                // or the computed edge lands far past the actual
                // screen (e.g. a 2560px-wide physical panel at
                // 1.35x scale is only ~1897 logical px wide).
                others.push((id, x, y));
                if have_any {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x + logical(raw_w));
                    max_y = max_y.max(y + logical(raw_h));
                } else {
                    have_any = true;
                    min_x = x;
                    min_y = y;
                    max_x = x + logical(raw_w);
                    max_y = y + logical(raw_h);
                }
            }
        }

        let Some((id, enabled, x, y, evdi_w, evdi_h)) = evdi else {
            continue;
        };

        // With nothing else on the desktop there is no edge to sit beside,
        // so the origin is as good an answer as any.
        let (want_x, want_y) = if !have_any {
            (0, 0)
        } else {
            use crate::config::Position::*;
            match position {
                Right => (max_x, min_y),
                Left => (min_x - evdi_w, min_y),
                Above => (min_x, min_y - evdi_h),
                Below => (min_x, max_y),
            }
        };

        // KDE will not take a negative position. It reports success and
        // then quietly leaves the output disabled — which is exactly what
        // "above" did before this: the command said ok and the virtual
        // screen simply never appeared. So when the virtual screen lands
        // above or left of everything, shift the whole layout back to the
        // origin instead, which is what the display settings panel does
        // when you drag a screen off the top-left corner.
        //
        // Normalising to exactly zero rather than merely to non-negative
        // keeps this idempotent: without it, switching to "above" once and
        // then back to "right" would leave the whole desktop permanently
        // offset downwards by the height of the virtual screen, and each
        // switch would add another band of empty space. Shifting every
        // output by the same amount changes no adjacency and nothing the
        // user can see — the coordinates are internal bookkeeping.
        let shift_x = -want_x.min(min_x);
        let shift_y = -want_y.min(min_y);
        let (want_x, want_y) = (want_x + shift_x, want_y + shift_y);
        let moves: Vec<(u32, i64, i64)> = others
            .iter()
            .map(|&(oid, ox, oy)| (oid, ox + shift_x, oy + shift_y))
            .collect();

        // Already where it belongs, and nothing else needs moving.
        // Re-issuing the command would make KWin reconfigure every output,
        // flickering the whole desktop — which is what used to happen on
        // each pass of the capture loop.
        if enabled && x == want_x && y == want_y && shift_x == 0 && shift_y == 0 {
            return;
        }

        info!(
            "Enabling EVDI output.{} at ({}, {}) — {:?} of the other screens",
            id, want_x, want_y, position
        );
        if shift_x != 0 || shift_y != 0 {
            info!(
                "  Shifting the other screens by ({}, {}) to keep the layout at the origin",
                shift_x, shift_y
            );
        }
        // All args in one kscreen-doctor call so KDE applies them atomically.
        // Position syntax is "X,Y" (comma-separated).
        let mut cmd = tokio::process::Command::new("kscreen-doctor");
        cmd.arg(format!("output.{}.enable", id))
            .arg(format!("output.{}.position.{},{}", id, want_x, want_y));
        if shift_x != 0 || shift_y != 0 {
            for (oid, nx, ny) in &moves {
                cmd.arg(format!("output.{}.position.{},{}", oid, nx, ny));
            }
        }
        let r = cmd.output().await;
        match r {
            Ok(o) if o.status.success() => info!("kscreen-doctor enable+position: ok"),
            Ok(o) => warn!(
                "kscreen-doctor failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => warn!("kscreen-doctor error: {}", e),
        }
        return;
    }

    warn!("EVDI output did not appear in kscreen-doctor within 3s");
}

// PLATFORM(kde): turn the EVDI output back off with kscreen-doctor
/// Switch the virtual output off again so windows left on it return to the real screens. Does nothing when
/// `kscreen-doctor` is missing or does not list an enabled output named in `evdi_names`.
pub async fn disable_output(evdi_names: &[String]) {
    let Ok(o) = tokio::process::Command::new("kscreen-doctor")
        .arg("-j")
        .output()
        .await
    else {
        return;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) else {
        return;
    };
    let Some(outputs) = v.get("outputs").and_then(|o| o.as_array()) else {
        return;
    };
    for out in outputs {
        let name = out.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if !evdi_names.iter().any(|n| n == name) {
            continue;
        }
        if !out
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let id = out.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        info!("Disabling EVDI output.{} ({})", id, name);
        let _ = tokio::process::Command::new("kscreen-doctor")
            .arg(format!("output.{}.disable", id))
            .output()
            .await;
    }
}

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
