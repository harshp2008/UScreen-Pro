//! Command-line interface of the `uscreen` binary: global stream overrides plus the subcommands
//! (`start`, `stop`, `status`, `doctor`, ...). Flags override `config.toml` for one run only.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Command-line argument specifications for the UScreen host service.
#[derive(Parser)]
#[command(
    name = "uscreen",
    version = env!("GIT_VERSION_STRING"),
    about = "USB second-screen server for Linux"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,

    #[arg(long = "display")]
    pub(crate) display: Option<String>,

    /// Explicit EDID override. By default an EDID is generated at runtime
    /// for the configured (or tablet-reported) resolution.
    #[arg(long = "edid")]
    pub(crate) edid: Option<PathBuf>,

    #[arg(long = "helper", default_value = "host/evdi/evdi_helper")]
    pub(crate) helper: PathBuf,

    #[arg(long = "auto-vdisplay", default_value_t = true)]
    pub(crate) auto_vdisplay: bool,

    /// Defaults come from ~/.config/uscreen/config.toml; CLI flags override.
    #[arg(long = "encoder")]
    pub(crate) encoder: Option<String>,

    #[arg(long = "fps")]
    pub(crate) fps: Option<u32>,

    #[arg(long = "bitrate")]
    pub(crate) bitrate: Option<u32>,

    #[arg(long = "width")]
    pub(crate) width: Option<u32>,

    #[arg(long = "height")]
    pub(crate) height: Option<u32>,

    #[arg(long = "quality")]
    pub(crate) quality: Option<u32>,

    /// Integer downscale for the stream only; the desktop keeps its native mode.
    #[arg(long = "stream-scale")]
    pub(crate) stream_scale: Option<u32>,

    /// Drive the laptop's own screen with the pen instead of streaming a second
    /// display to the tablet.
    #[arg(long = "pen-only")]
    pub(crate) pen_only: bool,

    #[arg(long = "video-port")]
    pub(crate) video_port: Option<u16>,

    #[arg(long = "input-port")]
    pub(crate) input_port: Option<u16>,

    /// Force 60 FPS stream on performance power profile.
    #[arg(long = "max-performance")]
    pub(crate) max_performance: bool,
}

/// Daemon operational subcommands.
#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Start the uscreen daemon
    Start {
        #[arg(long = "daemon", short = 'd')]
        daemonize: bool,
    },
    /// Stop the uscreen daemon
    Stop,
    /// Show daemon status
    Status,
    /// List available displays
    ListDisplays,
    /// Set the tablet up to connect over Wi-Fi, so the cable becomes optional
    Wifi {
        /// Forget the remembered address and stop reconnecting
        #[arg(long = "off")]
        off: bool,
    },
    /// Pause a device's display and capture session for surgical update
    PauseDevice { serial: String },
    /// Resume a device's display and capture session after surgical update
    ResumeDevice { serial: String },
    /// Diagnose the whole setup and report what is wrong
    Doctor {
        /// Instead: sweep a synthetic mouse across both screens for a few
        /// seconds and report the frame-path latency percentiles
        #[arg(long)]
        latency: bool,
        /// Length of the --latency run in seconds
        #[arg(long, default_value_t = 10)]
        secs: u64,
        /// With --latency: one small mouse step every 100 ms (isolated screen
        /// updates) instead of continuous motion
        #[arg(long)]
        sparse: bool,
    },
    /// Clear the compositor-safety record so the virtual display may be created
    /// again after a safe-mode start (restart the daemon afterwards)
    ResetGuard,
}
