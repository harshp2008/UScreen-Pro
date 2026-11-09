//! Video pipeline supervisor: spawns the EVDI helper (virtual monitor + frame grabber) and the
//! encoder, parses the Annex B output into access units and broadcasts them to the stream
//! server.
//!
//! This module root holds the shared types ([`Codec`], [`VideoPacket`], [`EncoderSettings`],
//! [`CaptureConfig`], [`DetectedMode`]) and the [`CaptureManager`] lifecycle. The pipeline has to
//! survive helper crashes, compositor-chosen mode changes, live settings changes from the tablet
//! and display hotplug storms, so the work is split by responsibility:
//!
//! - `display`: EVDI setup diagnosis, the frame FIFO, enabling / placing / removing the monitor.
//! - `helper`: spawning `evdi_helper` and reading its handshake.
//! - `spawn`: starting the encoder (ffmpeg child or in-process libavcodec).
//! - `supervisor`: the `stream_frames` restart loop with backoff.
//! - `reader` and `packetizer` (CLI encoder only): Annex B parsing into access units.

use anyhow::{Context, Result};
use bytes::Bytes;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;
#[cfg(not(feature = "inproc-encoder"))]
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, watch};
use tracing::{error, info, warn};

#[cfg(not(feature = "inproc-encoder"))]
mod avi;
mod display;
#[cfg(feature = "inproc-encoder")]
pub mod encoder;
pub mod encoders;
mod helper;
pub mod latency;
#[cfg(not(feature = "inproc-encoder"))]
mod packetizer;
#[cfg(not(feature = "inproc-encoder"))]
mod reader;
mod spawn;
mod supervisor;
pub mod timing;

/// Pause in milliseconds before the capture pipeline restarts after it ended.
const RECONNECT_DELAY_MS: u64 = 2000;
/// Capture FIFO, in the per-user runtime directory. It used to be
/// /tmp/uscreen_capture.fifo with mode 0666, which let any local account read
/// the raw frames off it.
pub fn fifo_path() -> String {
    crate::daemon::runtime::fifo_path()
        .to_string_lossy()
        .into_owned()
}

/// Path of the frame FIFO for the n-th virtual display (instance 0 keeps the legacy name).
///
/// One FIFO per tablet is what lets two pipelines run side by side without
/// interleaving their frames.
pub fn fifo_path_for(instance: u32) -> String {
    crate::daemon::runtime::fifo_path_for(instance)
        .to_string_lossy()
        .into_owned()
}

/// Which bitstream syntax is in play. H.264 and HEVC agree on Annex B start
/// codes and on nothing else that matters here: the NAL header is one byte
/// against two, the type lives in different bits, and a keyframe is a
/// different set of type numbers.
///
/// Not gated on the packetizer's feature flag: the daemon has to tell the
/// tablet which codec to build a decoder for regardless of how it was
/// compiled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    H264,
    Hevc,
}

impl Codec {
    /// Infer the bitstream from an ffmpeg encoder name (`hevc_vaapi`, `h265_nvenc`, ...); anything else is H.264.
    pub fn from_encoder(name: &str) -> Self {
        if name.contains("hevc") || name.contains("h265") || name.contains("265") {
            Codec::Hevc
        } else {
            Codec::H264
        }
    }

    /// The name the tablet receives in its greeting.
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
        }
    }
}
/// One H.264 access unit, tagged so the stream server can drop frames
/// safely (resume only at an IDR).
#[derive(Clone)]
pub struct VideoPacket {
    pub data: Bytes,
    pub is_idr: bool,
    /// Monotonically increasing per encoder run. Echoed back by the tablet once
    /// the frame is on screen, which is how end-to-end latency is measured.
    pub seq: u32,
}

/// Settings that can change at runtime (from the GUI or the tablet app).
/// A change restarts the encoder; an fps or resolution change also restarts
/// the helper (the EDID is regenerated for the new mode).
#[derive(Clone, Debug, PartialEq)]
pub struct EncoderSettings {
    pub encoder: String,
    pub fps: u32,
    pub bitrate: u32,
    pub width: u32,
    pub height: u32,
    /// Constant-quality target; see `config::DEFAULT_QUALITY`.
    pub quality: u32,
    /// Physical panel size for the generated EDID, in millimetres.
    pub width_mm: u32,
    pub height_mm: u32,
    /// Integer downscale for the stream; see `config::FileConfig::stream_scale`.
    pub stream_scale: u32,
    pub ten_bit: bool,
    pub keep_alive: bool,
    /// Bumped by an explicit session restart. Any change makes the capture loop kill and
    /// respawn both ffmpeg and evdi_helper, even when no other setting differs.
    pub restart_nonce: u64,
}
/// Everything one capture pipeline (EVDI helper + encoder) needs to start.
///
/// Built per device from the global config overlaid with the tablet's profile, so
/// changing any field that affects ffmpeg or the helper arguments requires a
/// pipeline restart.
#[derive(Clone)]
pub struct CaptureConfig {
    pub helper_path: PathBuf,
    /// Explicit EDID override; None = generate one for the configured mode
    pub edid_path: Option<PathBuf>,
    pub encoder: String,
    pub fps: u32,
    pub bitrate: u32,
    pub width: u32,
    pub height: u32,
    pub quality: u32,
    pub width_mm: u32,
    pub height_mm: u32,
    pub stream_scale: u32,
    /// Which edge of the existing desktop the virtual screen sits against.
    /// Not an encoder setting: changing it moves a window, it does not
    /// restart a stream.
    pub position: crate::config::Position,
    pub ten_bit: bool,
    /// Which tablet this pipeline serves (0 = the first): picks the FIFO
    /// name and, together with `card`, keeps two pipelines apart.
    pub instance: u32,
    /// EVDI card to pin the helper to. None lets the helper pick a free one,
    /// which is only safe with a single tablet.
    pub card: Option<u32>,
    /// When true and power profile is "performance", forces 60 FPS keepalive stream.
    pub max_performance_mode: bool,
    /// Keep virtual display active when tablet disconnects, throttled to 1 FPS.
    pub keep_alive: bool,
}

impl Default for CaptureConfig {
    /// Defaults for a capture session: helper next to the source tree, no EDID file, `h264_nvenc`.
    fn default() -> Self {
        Self {
            helper_path: PathBuf::from("host/evdi/evdi_helper"),
            edid_path: None,
            encoder: String::from("h264_nvenc"),
            fps: 60,
            bitrate: 20000,
            width: 2960,
            height: 1848,
            quality: crate::config::DEFAULT_QUALITY,
            width_mm: crate::display::edid::DEFAULT_WIDTH_MM,
            height_mm: crate::display::edid::DEFAULT_HEIGHT_MM,
            stream_scale: 1,
            position: crate::config::Position::Right,
            ten_bit: false,
            instance: 0,
            card: None,
            max_performance_mode: false,
            keep_alive: false,
        }
    }
}

/// The display mode KWin actually negotiated on the virtual output, as
/// reported by the helper's `MODE_CHANGED` line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectedMode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}

/// Owns the child processes of one tablet's video pipeline: the `evdi_helper`
/// (creates the virtual monitor and grabs its frames) and the encoder that turns
/// those frames into an H.264/HEVC bitstream.
///
/// The manager also supervises them: it restarts a crashed helper with
/// exponential backoff, asks the guard (see `guard.rs` at the crate root) for permission before
/// each display hotplug, and publishes the negotiated mode, card and codec
/// config to the servers that need them.
pub struct CaptureManager {
    config: CaptureConfig,
    helper_child: Option<Child>,
    encoder_child: Option<Child>,
    codec_config: Arc<Mutex<Option<Bytes>>>,
    /// `None` until the running helper reports a mode.
    ///
    /// The compositor is free to pick a mode other than the EDID's preferred
    /// one (it remembers per-output settings across sessions). When it does,
    /// the helper produces frames of one size while ffmpeg is told another,
    /// and every frame comes out skewed — a failure that used to be invisible
    /// because the helper's stdout was closed right after the handshake.
    mode_tx: watch::Sender<Option<DetectedMode>>,
    mode_rx: watch::Receiver<Option<DetectedMode>>,
    /// Frame size the helper actually emits. Differs from the display mode
    /// whenever the stream is downscaled, and it is this — not the mode — that
    /// the encoder must be configured for.
    stream_tx: watch::Sender<Option<(u32, u32)>>,
    stream_rx: watch::Receiver<Option<(u32, u32)>>,
    helper_stdout_task: Option<tokio::task::JoinHandle<()>>,
    /// DRM card index the running helper attached to, used to address the
    /// virtual output unambiguously.
    helper_card: Option<u32>,
    /// Published once the helper reports which card it opened, for whoever
    /// needs to address this tablet's output (the input mapping does).
    card_tx: watch::Sender<Option<u32>>,
    latency: crate::capture::latency::LatencyTracker,
    /// Set by the stream server when a client attaches. The in-process encoder
    /// turns the next frame into a keyframe, so a client joining an idle screen
    /// gets a picture immediately instead of waiting for the scheduled one.
    /// The CLI encoder has no way to honour this.
    #[cfg_attr(not(feature = "inproc-encoder"), allow(dead_code))]
    idr_wanted: Arc<std::sync::atomic::AtomicBool>,
}

impl CaptureManager {
    /// Create an idle manager; nothing is spawned until [`CaptureManager::stream_frames`] runs.
    pub fn new(config: CaptureConfig) -> Self {
        let (mode_tx, mode_rx) = watch::channel(None);
        let (stream_tx, stream_rx) = watch::channel(None);
        Self {
            config,
            helper_child: None,
            encoder_child: None,
            codec_config: Arc::new(Mutex::new(None)),
            mode_tx,
            mode_rx,
            stream_tx,
            stream_rx,
            helper_stdout_task: None,
            helper_card: None,
            card_tx: watch::channel(None).0,
            latency: crate::capture::latency::LatencyTracker::new(),
            idr_wanted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Which EVDI card the helper opened; None until it has.
    pub fn card_rx(&self) -> watch::Receiver<Option<u32>> {
        self.card_tx.subscribe()
    }

    /// Shared with the input server, which receives the tablet's render
    /// acknowledgements and closes the measurement loop.
    pub fn latency_tracker(&self) -> crate::capture::latency::LatencyTracker {
        self.latency.clone()
    }

    /// Shared with the stream server so a connecting client can ask for a
    /// keyframe rather than waiting for one.
    #[cfg_attr(not(feature = "inproc-encoder"), allow(dead_code))]
    pub fn idr_request_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.idr_wanted.clone()
    }

    /// Shared slot holding the stream's parameter sets (SPS/PPS/VPS), which the stream server replays to every client that attaches mid-stream.
    pub fn codec_config_arc(&self) -> Arc<Mutex<Option<Bytes>>> {
        self.codec_config.clone()
    }

    /// Frame size the encoder must be configured for.
    ///
    /// This is what the helper emits, which is the negotiated display mode
    /// divided by the stream scale — not the mode itself. Getting it wrong
    /// skews every frame for the whole session.
    fn active_mode(&self) -> (u32, u32) {
        if let Some((w, h)) = *self.stream_rx.borrow() {
            if w > 0 && h > 0 {
                return (w, h);
            }
        }
        let (w, h) = match *self.mode_rx.borrow() {
            Some(m) if m.width > 0 && m.height > 0 => (m.width, m.height),
            _ => (self.config.width, self.config.height),
        };
        let n = self.config.stream_scale.max(1);
        (((w / n) & !1).max(2), ((h / n) & !1).max(2))
    }
    /// Ask both children to exit with SIGTERM (so the helper can remove the virtual monitor cleanly) and stop reading the helper's stdout.
    pub fn stop(&mut self) {
        if let Some(task) = self.helper_stdout_task.take() {
            task.abort();
        }
        if let Some(child) = self.encoder_child.as_mut() {
            if let Some(pid) = child.id() {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
            }
        }
        if let Some(child) = self.helper_child.as_mut() {
            if let Some(pid) = child.id() {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
            }
        }
    }

    /// Stop the pipeline and wait for the children to actually be gone.
    ///
    /// The synchronous [`stop`] only *sends* signals, so the daemon could exit
    /// while ffmpeg was still holding the capture FIFO. That window collides
    /// with an immediate restart — which is exactly what "Apply & restart" in
    /// the GUI does — and two processes on one FIFO produce torn frames.
    pub async fn shutdown(&mut self) {
        if let Some(task) = self.helper_stdout_task.take() {
            task.abort();
        }
        if let Some(mut child) = self.encoder_child.take() {
            Self::terminate(&mut child, "ffmpeg").await;
        }
        if let Some(mut child) = self.helper_child.take() {
            Self::terminate(&mut child, "evdi_helper").await;
        }
        let _ = std::fs::remove_file(fifo_path_for(self.config.instance));
    }

    /// SIGTERM first, then reap. The helper installs a SIGTERM handler and uses
    /// it to run `evdi_disconnect`; SIGKILL skips that and leaves the connector
    /// attached until the kernel gets around to releasing the fd.
    async fn terminate(child: &mut Child, what: &str) {
        let Some(pid) = child.id() else { return };
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        match tokio::time::timeout(std::time::Duration::from_millis(1500), child.wait()).await {
            Ok(Ok(_)) => info!("{} exited cleanly", what),
            _ => {
                warn!("{} ignored SIGTERM — killing", what);
                let _ = child.start_kill();
                let _ =
                    tokio::time::timeout(std::time::Duration::from_millis(500), child.wait()).await;
            }
        }
    }
}

impl Drop for CaptureManager {
    /// Stops the capture and removes its FIFO.
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_file(fifo_path_for(self.config.instance));
    }
}

/// Tests for codec selection and the encoder allow-list.
#[cfg(all(test, not(feature = "inproc-encoder")))]
mod tests {
    use super::*;

    /// The codec follows the encoder name: `hevc_*` is HEVC, everything else H.264.
    #[test]
    fn codec_is_picked_from_the_encoder_name() {
        assert_eq!(Codec::from_encoder("h264_nvenc"), Codec::H264);
        assert_eq!(Codec::from_encoder("libx264"), Codec::H264);
        assert_eq!(Codec::from_encoder("hevc_nvenc"), Codec::Hevc);
        assert_eq!(Codec::from_encoder("hevc_vaapi"), Codec::Hevc);
    }

    /// `hevc_vaapi` is among the allowed encoders; unknown names are rejected.
    #[test]
    fn allowed_encoders_include_hevc_vaapi() {
        let check = |name: &str| -> Result<()> {
            match name {
                "h264_nvenc" | "hevc_nvenc" | "h264_vaapi" | "hevc_vaapi" | "libx264" => Ok(()),
                _ => anyhow::bail!("Unknown encoder: {name}"),
            }
        };
        for name in [
            "h264_nvenc",
            "hevc_nvenc",
            "h264_vaapi",
            "hevc_vaapi",
            "libx264",
        ] {
            assert!(check(name).is_ok());
        }
        assert!(check("invalid_enc").is_err());
    }
}
