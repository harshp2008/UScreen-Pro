//! Virtual display on KDE Plasma: switching the output on and off, placing it, and finding it, with `kscreen-doctor`.
//!
//! The D-Bus half of KWin (input device properties, the on-screen keyboard) is in [`super::kwin`]; its public functions are
//! re-exported here so other modules keep one path, `desktop::kde::display`.
// PLATFORM(kde): virtual display on KDE Plasma: kscreen-doctor for outputs; KWin D-Bus (busctl) is in kwin.rs

use tracing::{info, warn};

pub use super::kwin::{
    backend, get_property, in_kde_session, list_strings, set_property, KWIN_INPUT_IFACE,
};

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
