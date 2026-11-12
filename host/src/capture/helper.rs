//! Launching `evdi_helper`: the child that creates the EVDI virtual monitor and writes its raw
//! frames into the FIFO. Handles argument construction, the stdout handshake (card index and the
//! mode the compositor actually picked) and cleanup of a helper that failed to start.

use super::*;

/// Aborts the task it holds when dropped, so a helper-bound watcher dies with the helper's stdout task.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    /// Stop the held task.
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl CaptureManager {
    /// Start the EVDI helper process that writes the virtual display's frames into the FIFO, after
    /// killing any stray helper left from a previous run.
    pub(super) async fn start_helper(&mut self) -> Result<()> {
        let fifo = fifo_path_for(self.config.instance);
        Self::ensure_fifo(&fifo)?;

        // Kill any stray helper from a previous run before spawning a new one.
        // kill_on_drop only fires on a graceful exit; if the daemon was
        // SIGKILLed, pkill'd, or crashed, its helper is orphaned and keeps
        // writing full frames into the shared FIFO. Several such orphans
        // interleave their output, which the encoder reads as a single
        // stream — producing torn, banded frames mixing several captures.
        // Only a helper writing this session's FIFO is ours to kill: with
        // several tablets each has its own helper, and a plain
        // `pkill -x evdi_helper` here took down every other tablet's screen
        // whenever one of them (re)started. The daemon's own command line
        // never contains the FIFO path, so this cannot match the daemon.
        let killed_helper = Command::new("pkill")
            .args(["-f", &format!("evdi_helper .*--capture-fifo {}( |$)", fifo)])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        // A stray ffmpeg reading the same FIFO is just as bad as a stray
        // helper writing it — two readers/writers on one pipe interleave at
        // pipe granularity and corrupt frames. Match on the FIFO path so we
        // never touch an unrelated ffmpeg invocation.
        let killed_ffmpeg = Command::new("pkill")
            .args(["-f", &format!("ffmpeg.*{}", fifo)])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if killed_helper || killed_ffmpeg {
            warn!("Killed stray evdi_helper/ffmpeg process(es) before starting");
            // Give the kernel a moment to release the EVDI device(s) and
            // drop the old FIFO write end before we open a fresh one.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        // EDID 1.4 pixel-clock field is 16-bit (max 655 MHz).
        // 2960×1848 @120 Hz needs ~706 MHz which overflows.
        // Cap the EDID at 90 Hz so KDE can render at 90 fps; the helper
        // captures at the configured fps independently via clock_nanosleep.
        let edid_fps = self.config.fps.min(90);
        let edid_path = match &self.config.edid_path {
            Some(p) => p.clone(),
            None => {
                // `interface_size` changes the size the display claims, and with it the scale the compositor picks. The options are
                // re-read first: the save that asked for this restart may be newer than the last poll of the file.
                crate::config::options::refresh_all();
                let f = crate::display::edid::interface_factor(
                    &crate::config::options::current_options(self.config.instance).interface_size,
                );
                let scaled = |mm: u32| ((mm as f32 * f).round() as u32).clamp(1, 2550);
                crate::display::edid::ensure_edid_sized(
                    self.config.width,
                    self.config.height,
                    edid_fps,
                    scaled(self.config.width_mm),
                    scaled(self.config.height_mm),
                    self.config.instance + 1,
                )?
            }
        };

        let mut cmd = Command::new(&self.config.helper_path);
        cmd.args(["--edid", &edid_path.to_string_lossy()]);
        cmd.args(["--fps", &self.config.fps.to_string()]);
        if self.config.stream_scale > 1 {
            cmd.args(["--scale", &self.config.stream_scale.to_string()]);
        }

        cmd.args(["--capture-fifo", &fifo]);
        let timing_shm = crate::daemon::runtime::timing_path_for(self.config.instance);
        cmd.args(["--timing-shm", &timing_shm.to_string_lossy()]);
        if let Some(card) = self.config.card {
            cmd.args(["--card", &card.to_string()]);
        }

        if self.config.max_performance_mode {
            let profile = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                Command::new("powerprofilesctl").arg("get").output(),
            )
            .await;
            match profile {
                Ok(Ok(output)) if output.status.success() => {
                    let current = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!("Checked power profile: '{}'", current);
                    if current == "performance" {
                        info!("Max Performance Mode active (performance profile detected): appending --max-performance to evdi_helper arguments");
                        cmd.arg("--max-performance");
                    } else {
                        info!("Max Performance Mode configured, but power profile is '{}' (not 'performance') - running standard pacing", current);
                    }
                }
                _ => {
                    warn!("Failed or timed out querying powerprofilesctl");
                }
            }
        }

        // Mouse cursor mode, read fresh so a GUI change applies at the next respawn.
        // Default: composited cursor + the boundary-nudge window. Compatibility
        // mode: composited cursor only (USCREEN_CURSOR_EVENTS=0, no nudge).
        if crate::config::FileConfig::load().software_cursor {
            cmd.env("USCREEN_CURSOR_EVENTS", "0")
                .env("USCREEN_BOUNDARY_NUDGE", "0");
            info!("Software mouse cursor (compatibility) mode: boundary nudge disabled");
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .kill_on_drop(true);

        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                // Set evdi_helper to nice +1 to prioritize GNOME Mutter compositor
                libc::setpriority(libc::PRIO_PROCESS, 0, 1);
                Ok(())
            });
        }

        let mut child = cmd.spawn().context("Failed to spawn evdi-helper")?;
        // A fresh helper has not reported the pointer yet; never dim on stale information.
        crate::config::options::set_pointer(self.config.instance, true);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("No stdout from helper"))?;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        let card: u32;
        loop {
            match lines.next_line().await {
                Ok(Some(l)) => {
                    if let Some(rest) = l.strip_prefix("EVDI_CONNECTED card") {
                        card = rest.trim().parse()?;
                        info!("Helper connected on card{}", card);
                        break;
                    }
                }
                Ok(None) => {
                    anyhow::bail!("evdi-helper exited prematurely");
                }
                Err(e) => {
                    anyhow::bail!("evdi-helper stdout error: {}", e);
                }
            }
        }

        self.helper_card = Some(card);
        let _ = self.card_tx.send(Some(card));
        // A fresh helper has not negotiated a mode yet.
        let _ = self.mode_tx.send(None);
        let _ = self.stream_tx.send(None);

        // Keep draining stdout for the life of the helper. Dropping the reader
        // here (as this code used to) closes the pipe, so every later
        // MODE_CHANGED line dies with EPIPE and the host never learns what the
        // compositor really picked.
        if let Some(task) = self.helper_stdout_task.take() {
            task.abort();
        }
        let mode_tx = self.mode_tx.clone();
        let stream_tx = self.stream_tx.clone();
        let instance = self.config.instance;
        // On Wayland the compositor is asked where the pointer is (the helper's X pointer goes
        // stale over native windows); the watcher ends with this task.
        let compositor_pointer = crate::desktop::gnome::pointer::available();
        let mode_rx = self.mode_tx.subscribe();
        self.helper_stdout_task = Some(tokio::spawn(async move {
            let _watch = compositor_pointer.then(|| {
                AbortOnDrop(tokio::spawn(crate::desktop::gnome::pointer::watch_pointer(
                    instance, mode_rx,
                )))
            });
            while let Ok(Some(line)) = lines.next_line().await {
                // Without the compositor's answer, the helper watches the X pointer against this
                // output's rectangle and reports each crossing; the idle-dim timer starts when the
                // pointer leaves.
                if let Some(rest) = line.strip_prefix("POINTER_IN_TABLET ") {
                    if !compositor_pointer {
                        crate::config::options::set_pointer(instance, rest.trim() == "1");
                    }
                    continue;
                }
                if let Some(rest) = line.strip_prefix("STREAM_SIZE ") {
                    let mut p = rest.split_whitespace();
                    if let (Some(Ok(w)), Some(Ok(h))) = (
                        p.next().map(str::parse::<u32>),
                        p.next().map(str::parse::<u32>),
                    ) {
                        if w > 0 && h > 0 {
                            info!("Helper emits {}x{} frames to the encoder", w, h);
                            let _ = stream_tx.send(Some((w, h)));
                        }
                    }
                    continue;
                }
                let Some(rest) = line.strip_prefix("MODE_CHANGED ") else {
                    continue;
                };
                let mut parts = rest.split_whitespace();
                let parsed = (|| {
                    Some(DetectedMode {
                        width: parts.next()?.parse().ok()?,
                        height: parts.next()?.parse().ok()?,
                        refresh: parts.next()?.parse().ok()?,
                    })
                })();
                let Some(mode) = parsed else {
                    warn!("Unparseable MODE_CHANGED from helper: {}", rest);
                    continue;
                };
                if mode.width == 0 || mode.height == 0 {
                    continue;
                }
                info!(
                    "Compositor negotiated {}x{}@{}Hz on the virtual output",
                    mode.width, mode.height, mode.refresh
                );
                let _ = mode_tx.send(Some(mode));
            }
        }));

        self.helper_child = Some(child);
        Ok(())
    }
}
