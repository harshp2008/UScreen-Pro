//! The supervision loop of one pipeline.
//!
//! [`CaptureManager::stream_frames`] starts the helper and the encoder, forwards the packets they
//! produce, reacts to live settings changes, display on/off and shutdown, and restarts whichever
//! child dies, with exponential backoff so a crash-looping helper cannot flood the compositor
//! with hotplug events.

use super::display::evdi_setup_problem;
use super::*;

impl CaptureManager {
    /// Run the pipeline until shutdown: start the helper and encoder, forward encoded access units to `tx`, and restart whichever child dies.
    ///
    /// `settings_rx` carries live encoder changes from the tablet, `display_rx` is the
    /// on/off switch (false = tear the virtual display down), and `shutdown_rx` ends
    /// the loop. Restarts back off exponentially because a crash-looping helper
    /// generates display hotplug storms that can wedge the compositor.
    pub async fn stream_frames(
        &mut self,
        tx: broadcast::Sender<VideoPacket>,
        mut settings_rx: watch::Receiver<EncoderSettings>,
        mut display_rx: watch::Receiver<bool>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> Result<()> {
        // Exponential backoff: a crash-looping helper floods KWin with
        // display hotplug events, which can wedge the whole desktop.
        let mut backoff_ms: u64 = RECONNECT_DELAY_MS;
        let mut explained_evdi = false;
        let mut pipeline_started_at = Instant::now();
        let mut mode_rx = self.mode_rx.clone();
        let mut stream_rx = self.stream_rx.clone();
        // Touchpad and Stylus mode switch the whole pipeline off for this tablet, keep_alive included:
        // a throttled 1 fps stream would still run ffmpeg and the capture loop.
        let touchpad_rx = crate::config::options::options_rx(self.config.instance);
        // Frame size the running encoder was configured for, so a later mode
        // change can be detected as a mismatch rather than silently skewing.
        let mut encoder_mode: Option<(u32, u32)> = None;
        let mut is_throttled = false;
        let mut applied_nonce = settings_rx.borrow().restart_nonce;

        // Compositor safety (see crate::daemon::guard and experiments_and_findings/56_*):
        // the virtual monitor is only created once the compositor is up and idle,
        // hotplugs are spaced out, and a stall watchdog removes the monitor again
        // if the compositor stops answering while it exists.
        let mut gate = crate::daemon::guard::Guard::new();
        let (active_tx, active_rx) = watch::channel(false);
        let (mut trip_rx, mut ok_rx) = crate::daemon::guard::spawn_watchdog(active_rx);
        // keep_alive only means "keep the monitor after the tablet leaves". It
        // must never create one at login with nobody attached (issue #12): that
        // consumer-less monitor is what the desktop then has to lay windows,
        // docks and the pointer out on while it is still starting up.
        let mut tablet_seen = false;
        let mut refusal_logged = false;

        loop {
            tablet_seen |= *display_rx.borrow();
            // Apply the latest runtime settings before (re)starting anything
            {
                let s = settings_rx.borrow_and_update().clone();
                let forced_restart = s.restart_nonce != applied_nonce;
                applied_nonce = s.restart_nonce;
                // The physical size is baked into the EDID alongside the mode,
                // so a change there needs a fresh helper too.
                let needs_helper_restart = (s.fps != self.config.fps
                    || s.width != self.config.width
                    || s.height != self.config.height
                    || s.width_mm != self.config.width_mm
                    || s.height_mm != self.config.height_mm
                    || s.stream_scale != self.config.stream_scale
                    || forced_restart)
                    && self.helper_child.is_some();
                if needs_helper_restart {
                    // fps is baked into the helper's pacing, and the
                    // resolution into the EDID — restart with a fresh EDID
                    // Including the physical size: it goes into the EDID, so
                    // a change of a millimetre restarts the helper just as a
                    // resolution change does — and printing only w/h/fps made
                    // that look like a restart for no reason at all.
                    info!(
                        "Display mode change: {}x{}@{} ({}x{}mm) → {}x{}@{} ({}x{}mm)",
                        self.config.width,
                        self.config.height,
                        self.config.fps,
                        self.config.width_mm,
                        self.config.height_mm,
                        s.width,
                        s.height,
                        s.fps,
                        s.width_mm,
                        s.height_mm
                    );
                    if let Some(mut e) = self.encoder_child.take() {
                        Self::terminate(&mut e, "ffmpeg").await;
                    }
                    if let Some(mut h) = self.helper_child.take() {
                        Self::terminate(&mut h, "evdi_helper").await;
                        gate.display_down();
                        let _ = active_tx.send(false);
                    }
                    // Give the compositor a moment to process the unplug. The
                    // guard enforces the real minimum gap before the reconnect.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                self.config.encoder = s.encoder;
                self.config.fps = s.fps;
                self.config.bitrate = s.bitrate;
                self.config.width = s.width;
                self.config.height = s.height;
                self.config.quality = s.quality;
                self.config.width_mm = s.width_mm;
                self.config.height_mm = s.height_mm;
                self.config.stream_scale = s.stream_scale;
                self.config.ten_bit = s.ten_bit;
                self.config.keep_alive =
                    s.keep_alive && touchpad_rx.borrow().input_mode.streams_video();
            }

            // No tablet being used as a screen: no helper, and so no EVDI
            // connector for the desktop to see. Keeping the helper up "just
            // in case" put a connected-but-disabled monitor on the desktop
            // from the moment the daemon started, and KDE remembers layouts
            // by output — one user had set "show only on the UScreen screen"
            // once, and from then on every boot came up with the real screens
            // black and no tablet in sight (#12). Starting the helper only
            // while a tablet is attached makes the virtual monitor appear and
            // disappear exactly like a cable being plugged in and out.
            // If keep_alive is set, the helper and display are preserved.
            if !*display_rx.borrow() && !(self.config.keep_alive && tablet_seen) {
                info!("No tablet is a screen — disconnecting the virtual display");
                if let Some(mut e) = self.encoder_child.take() {
                    Self::terminate(&mut e, "ffmpeg").await;
                }
                if let Some(mut h) = self.helper_child.take() {
                    Self::terminate(&mut h, "evdi_helper").await;
                    gate.display_down();
                    let _ = active_tx.send(false);
                }
                encoder_mode = None;
                tokio::select! {
                    _ = display_rx.changed() => {}
                    _ = settings_rx.changed() => {}
                    _ = shutdown_rx.changed() => {
                        gate.display_down();
                        self.shutdown().await;
                        return Ok(());
                    }
                }
                continue;
            }

            // Ensure any exited child processes are reaped and cleared
            if let Some(ref mut child) = self.helper_child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        warn!("evdi_helper exited ({}); resetting for respawn", status);
                        self.helper_child = None;
                    }
                    Err(e) => {
                        warn!("evdi_helper wait error ({}): resetting for respawn", e);
                        self.helper_child = None;
                    }
                    Ok(None) => {}
                }
            }
            if let Some(ref mut child) = self.encoder_child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        warn!("Encoder process exited ({}); resetting for respawn", status);
                        self.encoder_child = None;
                    }
                    Err(e) => {
                        warn!("Encoder wait error ({}): resetting for respawn", e);
                        self.encoder_child = None;
                    }
                    Ok(None) => {}
                }
            }

            // Start evdi-helper if not running
            if self.helper_child.is_none() {
                // Creating the helper creates a monitor on the desktop. Ask the
                // guard first: it waits for the compositor instead of racing it.
                match gate.admit(&mut shutdown_rx).await {
                    crate::daemon::guard::Admission::Go => refusal_logged = false,
                    crate::daemon::guard::Admission::Shutdown => {
                        gate.display_down();
                        self.shutdown().await;
                        return Ok(());
                    }
                    crate::daemon::guard::Admission::Refused => {
                        if !refusal_logged {
                            refusal_logged = true;
                            warn!("guard: virtual display not created (safe mode / breaker latched); the tablet's pen and touch do not need it");
                        }
                        tokio::select! {
                            _ = shutdown_rx.changed() => {
                                self.shutdown().await;
                                return Ok(());
                            }
                            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                        }
                        continue;
                    }
                }
                if let Err(e) = self.start_helper().await {
                    error!(
                        "Failed to start helper: {}. Retrying in {}ms...",
                        e, backoff_ms
                    );
                    // Say why, once, instead of repeating an opaque line
                    // forever. Retrying is right for a transient failure and
                    // useless for a permissions problem, and the two look
                    // identical from here without asking.
                    if !explained_evdi {
                        if let Some(reason) = evdi_setup_problem() {
                            explained_evdi = true;
                            error!("{}", reason);
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(30_000);
                    continue;
                }
                explained_evdi = false;
                pipeline_started_at = Instant::now();
                gate.display_up();
                let _ = active_tx.send(true);
            }

            // Enable the display via kscreen-doctor so KWin actively renders
            // to it (which is what makes evdi_grab_pixels produce anything).
            //
            // Only while a tablet is attached and being used as a screen:
            // enabling it unconditionally puts a monitor on the desktop that
            // nobody can see, and KDE happily moves windows onto it. The
            // display_rx branch below enables it the moment that changes.
            if *display_rx.borrow() {
                Self::enable_evdi_display(self.helper_card, self.config.position).await;
            }

            // Wait (briefly) for the helper to report the mode the compositor
            // settled on before configuring ffmpeg's frame size. Guessing here
            // and getting it wrong yields a skewed picture for the whole
            // session, so a short wait is cheap insurance.
            //
            // Skipped when nothing is using the virtual output: it is
            // disabled then, so no mode is ever reported and the wait would
            // just add three seconds and a warning to every daemon start.
            if *display_rx.borrow() && self.stream_rx.borrow().is_none() {
                let mut wait_rx = self.stream_rx.clone();
                if tokio::time::timeout(std::time::Duration::from_secs(3), wait_rx.changed())
                    .await
                    .is_err()
                {
                    warn!(
                        "Compositor reported no mode within 3s — encoding at the requested {}x{}",
                        self.config.width, self.config.height
                    );
                }
            }
            mode_rx.borrow_and_update();
            stream_rx.borrow_and_update();

            // Start encoder if not running
            if self.encoder_child.is_none() {
                if let Err(e) = self.start_encoder().await {
                    error!(
                        "Failed to start encoder: {}. Retrying in {}ms...",
                        e, backoff_ms
                    );
                    if let Some(mut e) = self.encoder_child.take() {
                        Self::terminate(&mut e, "ffmpeg").await;
                    }
                    if let Some(mut h) = self.helper_child.take() {
                        Self::terminate(&mut h, "evdi_helper").await;
                        gate.display_down();
                        let _ = active_tx.send(false);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(30_000);
                    continue;
                }
                encoder_mode = Some(self.active_mode());
            }

            // The blocking encode loop cannot be aborted, so it is asked to
            // stop through a flag; the helper's keepalive guarantees it wakes
            // from the FIFO read a few times a second to notice.
            #[cfg(feature = "inproc-encoder")]
            let stop_encode = Arc::new(std::sync::atomic::AtomicBool::new(false));

            // Both encoder paths are driven as one task returning the same
            // type, because tokio::select! cannot take #[cfg] on its branches
            // and duplicating every arm to satisfy that would be worse.
            let mut encode_task: tokio::task::JoinHandle<Result<()>> = {
                #[cfg(not(feature = "inproc-encoder"))]
                {
                    let stdout = self
                        .encoder_child
                        .as_mut()
                        .unwrap()
                        .stdout
                        .take()
                        .ok_or_else(|| anyhow::anyhow!("Encoder has no stdout"))?;
                    let (tx2, cc, lat) =
                        (tx.clone(), self.codec_config.clone(), self.latency.clone());
                    let codec = Codec::from_encoder(&self.config.encoder);
                    let timing_shm = crate::daemon::runtime::timing_path_for(self.config.instance);
                    let instance = self.config.instance;
                    tokio::spawn(async move {
                        Self::read_loop(stdout, tx2, cc, lat, codec, timing_shm, instance).await
                    })
                }
                #[cfg(feature = "inproc-encoder")]
                {
                    let (w, h) = self.active_mode();
                    let (name, fps, bitrate, quality) = (
                        self.config.encoder.clone(),
                        self.config.fps,
                        self.config.bitrate,
                        self.config.quality,
                    );
                    // The in-process encoder feeds libavcodec NV12 straight
                    // from the FIFO, with no conversion step to hang 10-bit
                    // on. Say so rather than letting the setting quietly do
                    // nothing: a setting that is ignored in silence is worse
                    // than one that is refused out loud.
                    if self.config.ten_bit {
                        warn!(
                            "10-bit is not supported by the in-process encoder — \
                             encoding 8-bit. Build without --features inproc-encoder for 10-bit."
                        );
                    }
                    let (tx2, cc, idr, stopc, lat) = (
                        tx.clone(),
                        self.codec_config.clone(),
                        self.idr_wanted.clone(),
                        stop_encode.clone(),
                        self.latency.clone(),
                    );
                    tokio::task::spawn_blocking(move || {
                        crate::capture::encoder::run(
                            &fifo_path_for(self.config.instance),
                            &name,
                            w,
                            h,
                            fps,
                            bitrate,
                            quality,
                            tx2,
                            cc,
                            idr,
                            stopc,
                            lat,
                        )
                    })
                }
            };

            let mut settings_changed = false;
            // Distinct from `settings_changed`: the mode moved under us, so the
            // encoder must be rebuilt but the helper and the virtual display
            // are fine and must not be torn down.
            let mut mode_changed = false;
            // The tablet stopped being a screen: a clean stop, not a crash,
            // so no backoff and no wait before the outer loop parks itself.
            let mut display_dropped = false;
            // The stall watchdog fired: take the virtual monitor away again.
            let mut guard_tripped = false;
            let card = self.helper_card;

            // Events that need no restart at all send us back here without
            // rebuilding the encoder, which now owns the stream and must not be
            // torn down for something spurious.
            #[allow(unused_labels)]
            'session: loop {
                let mut resume_same_encoder = false;
                tokio::select! {
                    joined = &mut encode_task => {
                        match joined {
                            Ok(Ok(_)) => info!("Encoder finished"),
                            Ok(Err(e)) => {
                                warn!("Encoder error: {}. Restarting...", e);
                                if self.config.encoder == "hevc_vaapi" {
                                    warn!("hevc_vaapi failed during streaming; falling back to h264_vaapi");
                                    self.config.encoder = "h264_vaapi".to_string();
                                }
                            }
                            Err(e) => {
                                warn!("Encoder task failed: {}. Restarting...", e);
                                if self.config.encoder == "hevc_vaapi" {
                                    warn!("hevc_vaapi failed during streaming; falling back to h264_vaapi");
                                    self.config.encoder = "h264_vaapi".to_string();
                                }
                            }
                        }
                    }
                    _ = settings_rx.changed() => {
                        info!("Settings changed — restarting encoder");
                        settings_changed = true;
                    }
                    _ = trip_rx.changed() => {
                        warn!("guard: removing the virtual display - the compositor stopped answering");
                        guard_tripped = true;
                    }
                    _ = ok_rx.changed() => {
                        gate.healthy();
                        resume_same_encoder = true;
                    }
                    _ = stream_rx.changed() => {
                        let now = self.active_mode();
                        if Some(now) == encoder_mode {
                            resume_same_encoder = true;
                        } else {
                            info!("Stream size is now {}x{} — restarting encoder", now.0, now.1);
                            mode_changed = true;
                        }
                    }
                    _ = mode_rx.changed() => {
                        let now = self.active_mode();
                        if Some(now) == encoder_mode {
                            // KWin re-applying the same mode. Nothing to do.
                            resume_same_encoder = true;
                        } else {
                            info!(
                                "Virtual output changed to {}x{} — restarting encoder to match",
                                now.0, now.1
                            );
                            mode_changed = true;
                        }
                    }
                    _ = display_rx.changed() => {
                        // The tablet stopped being a screen — either unplugged, or
                        // switched to pen-only. Neither must leave a monitor behind
                        // that nobody can see, with windows stranded on it. The
                        // encoder itself is unaffected either way.
                        let wanted = *display_rx.borrow();
                        if wanted {
                            info!("Tablet is a screen — enabling the virtual display");
                            Self::enable_evdi_display(card, self.config.position).await;
                            is_throttled = false;
                            if let Some(pid) = self.helper_child.as_ref().and_then(|c| c.id()) {
                                unsafe {
                                    libc::kill(pid as libc::pid_t, libc::SIGUSR2);
                                }
                            }
                            self.idr_wanted.store(true, std::sync::atomic::Ordering::SeqCst);
                            info!("Restored target capture FPS and triggered full keyframe refresh (client active)");
                            resume_same_encoder = true;
                        } else {
                            if self.config.keep_alive && touchpad_rx.borrow().input_mode.streams_video() {
                                info!("Tablet disconnected but keep_alive is enabled — maintaining virtual display and throttling capture to 1 FPS");
                                if !is_throttled {
                                    is_throttled = true;
                                    if let Some(pid) = self.helper_child.as_ref().and_then(|c| c.id()) {
                                        unsafe {
                                            libc::kill(pid as libc::pid_t, libc::SIGUSR1);
                                        }
                                    }
                                }
                                resume_same_encoder = true;
                            } else {
                                // Disable first so KWin moves the windows off it, then
                                // fall through to the teardown: the helper goes away
                                // with the session, and the top of the outer loop
                                // waits for a tablet before bringing anything back.
                                info!("Tablet is not a screen — disabling the virtual display");
                                Self::disable_evdi_display(card).await;
                                display_dropped = true;
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("Shutdown requested — tearing down the capture pipeline");
                        // Drop the encode task, which closes our read end of the
                        // pipe. Nothing drains it during shutdown, so ffmpeg would
                        // otherwise block writing into a full pipe and never reach
                        // its signal handling — a wasted 1.5s SIGTERM timeout on
                        // every stop. Closed, it gets EPIPE and exits at once.
                        #[cfg(feature = "inproc-encoder")]
                        stop_encode.store(true, std::sync::atomic::Ordering::Relaxed);
                        encode_task.abort();
                        Self::disable_evdi_display(card).await;
                        gate.display_down();
                        self.shutdown().await;
                        return Ok(());
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
                        let mut needs_respawn = false;
                        if let Some(ref mut child) = self.helper_child {
                            if let Ok(Some(st)) = child.try_wait() {
                                warn!("evdi_helper exited in background ({}); scheduling respawn", st);
                                self.helper_child = None;
                                needs_respawn = true;
                            }
                        }
                        if let Some(ref mut child) = self.encoder_child {
                            if let Ok(Some(st)) = child.try_wait() {
                                warn!("Encoder exited in background ({}); scheduling respawn", st);
                                self.encoder_child = None;
                                needs_respawn = true;
                            }
                        }
                        if needs_respawn {
                            break;
                        }
                        resume_same_encoder = true;
                    }
                }

                if resume_same_encoder {
                    // Nothing about the encoder changed, so it keeps running and we
                    // simply go back to waiting on it. The task owns the stream, so
                    // it must not be torn down and rebuilt for a spurious event.
                    continue 'session;
                }

                // Wind the encoder down before rebuilding it.
                #[cfg(feature = "inproc-encoder")]
                {
                    stop_encode.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = (&mut encode_task).await;
                }
                encode_task.abort();

                break;
            }

            // A pipeline that ran for a while was healthy — reset the backoff.
            // A pipeline that died within seconds is crash-looping — back off.
            if pipeline_started_at.elapsed().as_secs() >= 30 {
                backoff_ms = RECONNECT_DELAY_MS;
            } else if !settings_changed && !mode_changed && !display_dropped {
                backoff_ms = (backoff_ms * 2).min(30_000);
            }

            // Clean up and retry. On a settings or mode change, keep the helper
            // alive (an fps/resolution change is handled at the top of the loop)
            // so the virtual display doesn't flicker off.
            if let Some(mut e) = self.encoder_child.take() {
                Self::terminate(&mut e, "ffmpeg").await;
            }
            if !settings_changed && !mode_changed {
                if let Some(mut h) = self.helper_child.take() {
                    Self::terminate(&mut h, "evdi_helper").await;
                    if guard_tripped {
                        gate.tripped();
                    } else {
                        gate.display_down();
                    }
                    let _ = active_tx.send(false);
                }
            }
            // A helper that died on its own is gone too: stop probing for it.
            if self.helper_child.is_none() {
                let _ = active_tx.send(false);
            }
            encoder_mode = None;
            // Reset codec config so it gets re-extracted on restart
            if let Ok(mut config) = self.codec_config.lock() {
                *config = None;
            }
            if !settings_changed && !mode_changed && !display_dropped && !guard_tripped {
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            }
        }
    }
}
