//! Placement of the virtual (EVDI) display on the desktop, and the FIFO the helper writes frames to.
//!
//! Why this is separate from the supervisor: every compositor (KWin, Mutter, Hyprland, wlroots,
//! X11) wants a different incantation to enable, position and later remove the new output, and
//! none of it has anything to do with encoding. Keeping it here lets the supervisor stay a
//! plain "start helper, start encoder, forward frames" loop.

use super::*;

/// Why the helper cannot get an EVDI device, in the words of someone who can
/// fix it. Returns None when the state looks fine and the failure was
/// something else.
pub(super) fn evdi_setup_problem() -> Option<String> {
    evdi_setup_problem_in(std::path::Path::new("/sys/devices/evdi"))
}

/// Split out so it can be tested against a temporary directory. Unloading the
/// real module needs root and takes Xwayland down with it, so the three states
/// this has to tell apart are otherwise unreachable from a test.
fn evdi_setup_problem_in(dir: &std::path::Path) -> Option<String> {
    let module_loaded = dir.exists();
    if !module_loaded {
        return Some(
            "The evdi kernel module is not loaded. Install it (evdi-dkms; on Arch it is in \
             the AUR) and run: sudo modprobe evdi"
                .to_string(),
        );
    }

    let count: u32 = std::fs::read_to_string(dir.join("count"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if count > 0 {
        return None;
    }

    // The module is there but has no devices, and creating one needs a write
    // to a root-only file. `initial_device_count` fixes it for good, but only
    // takes effect when the module loads — which is why writing the
    // modprobe.d file is not enough on a machine where evdi is already
    // resident.
    Some(
        "No EVDI device exists and /sys/devices/evdi/add is root-only, so one cannot be \
         created. Fix it for this boot with:\n    echo 1 | sudo tee /sys/devices/evdi/add\n\
         and for every boot with:\n    echo 'options evdi initial_device_count=2' | sudo tee \
         /etc/modprobe.d/uscreen-evdi.conf\n    sudo modprobe -r evdi && sudo modprobe evdi\n\
         Then run: uscreen doctor"
            .to_string(),
    )
}

impl CaptureManager {
    pub(super) fn ensure_fifo(path: &str) -> Result<()> {
        // Anything already there is replaced, whatever it is: a stale FIFO
        // may carry old permissions, and a regular file or symlink in this
        // spot is not ours. mkfifo itself never follows symlinks.
        let _ = std::fs::remove_file(path);
        let c = std::ffi::CString::new(path).context("fifo path")?;
        // 0600: the helper writes it and the encoder reads it, both as this
        // user. Nobody else has any business with a live copy of the screen.
        let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error()).context("mkfifo");
        }
        Ok(())
    }

    /// Enable the EVDI output and place it to the right of every other screen.
    ///
    /// The output is identified by the DRM connector names sysfs reports for
    /// EVDI cards, not by whether the name happens to contain "DVI": a real DVI
    /// monitor on a dock produces exactly the same name pattern, and acting on
    /// it would enable and move the user's physical screen instead of ours.
    ///
    /// The position is derived at runtime from the existing display geometry so
    /// it works regardless of the laptop's screen resolution or scaling factor.
    pub(super) async fn enable_evdi_display(card: Option<u32>, position: crate::config::Position) {
        let evdi_names: Vec<String> = crate::vdisplay::evdi_connectors()
            .into_iter()
            .filter(|c| card.is_none_or(|want| c.card == want))
            .map(|c| c.name)
            .collect();
        if evdi_names.is_empty() {
            warn!("No EVDI connector found in sysfs — cannot enable the virtual display");
            return;
        }
        if crate::hyprland::active() {
            crate::hyprland::enable_output(&evdi_names, position).await;
            return;
        }
        if crate::cosmic::active() {
            crate::cosmic::enable_output(&evdi_names).await;
            return;
        }
        // GNOME places and switches on a new monitor by itself.
        if crate::gnome::active() {
            return;
        }
        if crate::x11::active() {
            crate::x11::enable_output(&evdi_names, position).await;
            return;
        }

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

    /// Turn the virtual output back off so its windows return to the real
    /// screens. Without this the desktop keeps a monitor nobody can see after
    /// the tablet is unplugged, and windows left there are effectively lost.
    pub async fn disable_evdi_display(card: Option<u32>) {
        let evdi_names: Vec<String> = crate::vdisplay::evdi_connectors()
            .into_iter()
            .filter(|c| card.is_none_or(|want| c.card == want))
            .map(|c| c.name)
            .collect();
        if evdi_names.is_empty() {
            return;
        }
        if crate::hyprland::active() {
            crate::hyprland::disable_output(&evdi_names).await;
            return;
        }
        if crate::cosmic::active() {
            crate::cosmic::disable_output(&evdi_names).await;
            return;
        }
        if crate::gnome::active() {
            return;
        }
        if crate::x11::active() {
            crate::x11::disable_output(&evdi_names).await;
            return;
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdi_problem_reports_a_missing_module() {
        let dir = std::env::temp_dir().join("uscreen-test-evdi-absent");
        let _ = std::fs::remove_dir_all(&dir);
        let msg = evdi_setup_problem_in(&dir).expect("absent module is a problem");
        assert!(msg.contains("not loaded"), "got: {msg}");
    }
    #[test]
    fn evdi_problem_reports_a_module_with_no_devices() {
        // The state the Arch report landed in: module resident, count 0, and
        // /sys/devices/evdi/add writable only by root.
        let dir = std::env::temp_dir().join("uscreen-test-evdi-empty");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("count"), "0\n").unwrap();
        let msg = evdi_setup_problem_in(&dir).expect("no devices is a problem");
        assert!(msg.contains("initial_device_count"), "got: {msg}");
        assert!(
            msg.contains("modprobe -r evdi"),
            "must give the reload, got: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn evdi_problem_stays_quiet_when_a_device_exists() {
        let dir = std::env::temp_dir().join("uscreen-test-evdi-ok");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("count"), "1\n").unwrap();
        assert!(
            evdi_setup_problem_in(&dir).is_none(),
            "a working setup must not be blamed for an unrelated failure"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
