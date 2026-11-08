//! Starting the encoder: either an `ffmpeg` child process reading the FIFO (the default CLI path)
//! or libavcodec in-process (`inproc-encoder` feature). Also builds the per-encoder argument
//! lists (NVENC, VAAPI, libx264, HEVC variants).

use super::*;

impl CaptureManager {
    /// With the in-process encoder there is no child to spawn; the encode loop
    /// is started per session instead.
    #[cfg(feature = "inproc-encoder")]
    pub(super) async fn start_encoder(&mut self) -> Result<()> {
        Ok(())
    }

    #[cfg(not(feature = "inproc-encoder"))]
    pub(super) async fn spawn_encoder_child(&self, encoder: &str) -> Result<Child> {
        let res: Result<()> = match encoder {
            "h264_nvenc" | "hevc_nvenc" | "h264_vaapi" | "hevc_vaapi" | "libx264" => Ok(()),
            _ => anyhow::bail!(
                "Unknown encoder: {}. Use h264_nvenc, hevc_nvenc, h264_vaapi, hevc_vaapi, or libx264",
                encoder
            ),
        };
        res?;

        let (w, h) = self.active_mode();
        let fps = self.config.fps;
        let bitrate = self.config.bitrate;
        let codec = Codec::from_encoder(encoder);
        // 10-bit only makes sense on HEVC here: NVENC's H.264 encoder is
        // 8-bit, so asking for it there would silently do nothing.
        let ten_bit = self.config.ten_bit && codec == Codec::Hevc;
        if self.config.ten_bit && !ten_bit {
            warn!(
                "10-bit was asked for but {} is 8-bit only — ignoring",
                encoder
            );
        }
        // Keyframe every second: enough for fast client joins without
        // burning the whole bitrate budget on IDR frames.
        let gop = fps.max(1);

        let mut encoder_args: Vec<String> = vec!["-hide_banner".into()];

        if encoder == "h264_vaapi" || encoder == "hevc_vaapi" {
            encoder_args
                .extend_from_slice(&["-vaapi_device".into(), crate::encoders::vaapi_device()]);
        }

        encoder_args.extend_from_slice(&[
            "-fflags".into(),
            "nobuffer".into(),
            "-flags".into(),
            "low_delay".into(),
            // The helper emits BT.709 limited-range NV12. Say so on the
            // INPUT side, before -i. Given as output options (where they used
            // to be), ffmpeg 7+ treats them as a request to *convert* an
            // untagged input to BT.709, auto-inserts a scale filter and runs
            // every frame through swscale via an RGB intermediate on the CPU:
            // measured ~4 cores at 60 fps, 2960x1848, against 0.4 without,
            // plus a full-frame conversion's worth of latency and a colour
            // shift from treating 709 data as 601. Tagged on the input, the
            // stream still carries bt709/tv in the SPS and nothing is
            // converted. Full range was tried and reverted — see the note in
            // evdi_helper.c.
            "-color_primaries".into(),
            "bt709".into(),
            "-color_trc".into(),
            "bt709".into(),
            "-colorspace".into(),
            "bt709".into(),
            "-color_range".into(),
            "tv".into(),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            // The helper converts the BGRA framebuffer to NV12 before the
            // FIFO: 1.5 bytes/px instead of 4, so the raw-frame copies that
            // bottleneck the pipeline shrink 2.7x and NVENC takes it directly.
            "nv12".into(),
            "-s".into(),
            format!("{}x{}", w, h),
            "-framerate".into(),
            fps.to_string(),
            "-i".into(),
            fifo_path_for(self.config.instance),
        ]);

        if encoder == "h264_vaapi" || encoder == "hevc_vaapi" {
            // For VAAPI hardware encode, directly upload the 8-bit NV12 frames from the capture FIFO.
            // Avoid software CPU conversion to p010le which burns 240% CPU and starves the compositor.
            encoder_args.extend_from_slice(&["-vf".into(), "format=nv12,hwupload".into()]);
        } else if ten_bit {
            // The FIFO carries 8-bit NV12, so 10-bit encoding needs a conversion
            // first. Done here rather than in the helper to keep the FIFO format
            // single: the helper stays the one thing that never has to know which
            // codec is in use.
            encoder_args.extend_from_slice(&["-vf".into(), "format=p010le".into()]);
        }

        encoder_args.extend_from_slice(&["-c:v".into(), encoder.to_string()]);

        if encoder.ends_with("_nvenc") {
            let bitrate_m = bitrate as f64 / 1000.0;
            // bufsize = 1 frame of bits: keeps VBV under 1-frame delay.
            let bufsize_k = (bitrate / fps.max(1)).max(200);
            encoder_args.extend_from_slice(&[
                "-preset".into(),
                "p1".into(),
                "-tune".into(),
                "ull".into(),
                "-zerolatency".into(),
                "1".into(),
                "-delay".into(),
                "0".into(),
                "-bf".into(),
                "0".into(),
                "-rc-lookahead".into(),
                "0".into(),
                "-multipass".into(),
                "0".into(),
                // Constant-quality VBR, not CBR.
                //
                // CBR pads every frame to hit the target rate, so a completely
                // motionless desktop still pushed the full bitrate down the USB
                // link — measured at 7.5 MB/s with nothing moving on screen.
                // That traffic buys nothing and leaves no headroom for the
                // moments that do need it. With `-b:v 0` plus `-cq`, NVENC
                // spends bits only where the picture actually changes and
                // `-maxrate` still caps the bursts.
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                self.config.quality.to_string(),
                "-b:v".into(),
                "0".into(),
                "-maxrate".into(),
                format!("{:.1}M", bitrate_m),
                "-bufsize".into(),
                format!("{}k", bufsize_k),
                "-g".into(),
                gop.to_string(),
                "-forced-idr".into(),
                "1".into(),
            ]);
            if ten_bit {
                // The source is 8-bit — EVDI hands over ARGB8888 and there is
                // no 10-bit path below us — so this adds no colour the desktop
                // did not have. What it buys is precision in the encoder's own
                // arithmetic: quantisation and motion compensation round in
                // 10 bits instead of 8, which is what smooths the banding that
                // shows up on gradients at low bitrates.
                encoder_args.extend_from_slice(&["-profile:v".into(), "main10".into()]);
            }
        } else if encoder == "h264_vaapi" || encoder == "hevc_vaapi" {
            // Constant quality, for the same reason as NVENC above: a static
            // desktop should cost nothing, and the bitrate is only a ceiling.
            encoder_args.extend_from_slice(&[
                "-rc_mode".into(),
                "CQP".into(),
                "-qp".into(),
                self.config.quality.to_string(),
                "-maxrate".into(),
                format!("{}k", bitrate),
                "-bf".into(),
                "0".into(),
                "-g".into(),
                gop.to_string(),
                "-idr_interval".into(),
                "1".into(),
            ]);
            // The default of 2 keeps each frame inside the encoder until the next one
            // arrives. Older ffmpeg (before 5.0) has no such queue and rejects the option,
            // so ask before passing it.
            if crate::encoders::has_option(encoder, "async_depth").await {
                encoder_args.extend_from_slice(&["-async_depth".into(), "1".into()]);
            }
            if encoder == "hevc_vaapi" {
                // Main profile uses direct 8-bit NV12 hardware encoding on Intel Arc
                encoder_args.extend_from_slice(&["-profile:v".into(), "main".into()]);
                // Only hevc_vaapi has these knobs (hevc_nvenc is always run
                // zero-latency above). Read fresh: a GUI toggle respawns ffmpeg.
                if crate::config::FileConfig::load().hevc_low_latency_tuning {
                    encoder_args.extend_from_slice(&[
                        "-b_depth".into(),
                        "1".into(),
                        "-sei".into(),
                        "0".into(),
                        "-aud".into(),
                        "0".into(),
                    ]);
                }
            }
        } else if encoder == "libx264" {
            let bufsize_k = (bitrate * 2 / fps.max(1)).max(200);
            encoder_args.extend_from_slice(&[
                "-preset".into(),
                "ultrafast".into(),
                "-tune".into(),
                "zerolatency".into(),
                "-crf".into(),
                self.config.quality.to_string(),
                "-maxrate".into(),
                format!("{}k", bitrate),
                "-bufsize".into(),
                format!("{}k", bufsize_k),
                "-g".into(),
                gop.to_string(),
                "-x264-params".into(),
                "scenecut=0".into(),
            ]);
        } else {
            anyhow::bail!(
                "Unknown encoder: {}. Use h264_nvenc, hevc_nvenc, h264_vaapi, hevc_vaapi, or libx264",
                encoder
            );
        }

        // AVI rather than raw Annex B: Annex B only shows where a frame ends when a
        // later frame starts, which held every frame back for two more frames (up to
        // 400 ms on a quiet screen). An AVI chunk states its size, so a frame leaves
        // the moment it is complete. See `avi.rs`.
        encoder_args.extend_from_slice(&[
            "-flush_packets".into(),
            "1".into(),
            "-f".into(),
            "avi".into(),
            "pipe:1".into(),
        ]);

        let mut cmd = Command::new("ffmpeg");
        cmd.args(&encoder_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .kill_on_drop(true);

        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                // Lower FFmpeg CPU priority (nice +5) so Mutter and desktop presentation are never starved
                libc::setpriority(libc::PRIO_PROCESS, 0, 5);
                Ok(())
            });
        }

        cmd.spawn().context("Failed to spawn ffmpeg encoder")
    }

    #[cfg(not(feature = "inproc-encoder"))]
    pub(super) async fn start_encoder(&mut self) -> Result<()> {
        // Accept the old gstreamer-style name as an alias
        let encoder = if self.config.encoder == "vaapih264enc" {
            "h264_vaapi".to_string()
        } else {
            self.config.encoder.clone()
        };
        // Always the size the helper really emits, never the one we asked for:
        // a mismatch here shows up as a permanently skewed picture.
        let (w, h) = self.active_mode();
        let n = self.config.stream_scale.max(1);
        let expected = (
            ((self.config.width / n) & !1).max(2),
            ((self.config.height / n) & !1).max(2),
        );
        if (w, h) != expected {
            warn!(
                "Encoding at {}x{}, expected {}x{} — the compositor did not honour \
                 the requested mode",
                w, h, expected.0, expected.1
            );
        } else if n > 1 {
            info!(
                "Encoding at {}x{} (desktop {}x{}, stream scale {})",
                w, h, self.config.width, self.config.height, n
            );
        }

        let child = match self.spawn_encoder_child(&encoder).await {
            Ok(child) => child,
            Err(e) => {
                if encoder == "hevc_vaapi" {
                    warn!(
                        "Encoder hevc_vaapi failed to initialize ({}). Attempting graceful fallback to h264_vaapi",
                        e
                    );
                    self.config.encoder = "h264_vaapi".to_string();
                    self.spawn_encoder_child("h264_vaapi")
                        .await
                        .context("Failed to spawn fallback h264_vaapi encoder")?
                } else {
                    return Err(e);
                }
            }
        };

        info!("Encoder started (PID: {})", child.id().unwrap_or(0));
        self.encoder_child = Some(child);
        Ok(())
    }
}
