mod capture;
mod config;
mod cosmic;
mod doctor;
mod edid;
#[cfg(feature = "inproc-encoder")]
mod encoder;
mod encoders;
mod gnome;
mod hyprland;
mod kwin;
mod input;
mod latency;
mod osk;
mod runtime;
mod stream;
pub mod timing;
mod tray;
mod update;
mod vdisplay;
mod x11;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokio::signal;
use tokio::sync::{broadcast, watch, mpsc, oneshot};
use tracing::{error, info, warn};

#[derive(Parser)]
#[command(
    name = "uscreen",
    version = env!("GIT_VERSION_STRING"),
    about = "USB second-screen server for Linux"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long = "display")]
    display: Option<String>,

    /// Explicit EDID override. By default an EDID is generated at runtime
    /// for the configured (or tablet-reported) resolution.
    #[arg(long = "edid")]
    edid: Option<PathBuf>,

    #[arg(long = "helper", default_value = "host/evdi/evdi_helper")]
    helper: PathBuf,

    #[arg(long = "auto-vdisplay", default_value_t = true)]
    auto_vdisplay: bool,

    /// Defaults come from ~/.config/uscreen/config.toml; CLI flags override.
    #[arg(long = "encoder")]
    encoder: Option<String>,

    #[arg(long = "fps")]
    fps: Option<u32>,

    #[arg(long = "bitrate")]
    bitrate: Option<u32>,

    #[arg(long = "width")]
    width: Option<u32>,

    #[arg(long = "height")]
    height: Option<u32>,

    #[arg(long = "quality")]
    quality: Option<u32>,

    /// Integer downscale for the stream only; the desktop keeps its native mode.
    #[arg(long = "stream-scale")]
    stream_scale: Option<u32>,

    /// Drive the laptop's own screen with the pen instead of streaming a second
    /// display to the tablet.
    #[arg(long = "pen-only")]
    pen_only: bool,

    #[arg(long = "video-port")]
    video_port: Option<u16>,

    #[arg(long = "input-port")]
    input_port: Option<u16>,

    /// Force 60 FPS stream on performance power profile
    #[arg(long = "max-performance")]
    max_performance: bool,
}

#[derive(Subcommand)]
enum Commands {
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
    PauseDevice {
        serial: String,
    },
    /// Resume a device's display and capture session after surgical update
    ResumeDevice {
        serial: String,
    },
    /// Diagnose the whole setup and report what is wrong
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logging();

    match &cli.command {
        Some(Commands::Start { daemonize }) if *daemonize => {
            let exe = std::env::current_exe()?;
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("start");
            let home = std::env::var("HOME").unwrap_or_default();
            let log_dir = std::path::PathBuf::from(format!("{}/.local/share/uscreen", home));
            let _ = std::fs::create_dir_all(&log_dir);
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_dir.join("daemon.log"))?;
            let log_err = log.try_clone()?;
            cmd.stdout(log);
            cmd.stderr(log_err);
            cmd.stdin(std::process::Stdio::null());
            #[cfg(unix)]
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(|| {
                    libc::setsid();
                    libc::signal(libc::SIGHUP, libc::SIG_IGN);
                    Ok(())
                });
            }
            let child = cmd.spawn()?;
            println!("uscreen daemon started in background (PID: {})", child.id());
            return Ok(());
        }
        Some(Commands::Start { .. }) | None => {
            info!("Starting uscreen daemon v{} (PID: {})", env!("GIT_VERSION_STRING"), std::process::id());
            run_daemon(cli).await?;
        }
        Some(Commands::Stop) => stop_daemon().await?,
        Some(Commands::Status) => show_status().await?,
        Some(Commands::PauseDevice { serial }) => client_control_cmd(&format!("PAUSE {}", serial)).await?,
        Some(Commands::ResumeDevice { serial }) => client_control_cmd(&format!("RESUME {}", serial)).await?,
        Some(Commands::ListDisplays) => list_displays().await?,
        Some(Commands::Wifi { off }) => setup_wifi(*off).await?,
        Some(Commands::Doctor) => doctor::run().await?,
    }

    Ok(())
}

pub enum DaemonControlMsg {
    PauseDevice {
        serial: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    ResumeDevice {
        serial: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
}

async fn client_control_cmd(cmd: &str) -> Result<()> {
    let control_sock_path = crate::runtime::runtime_dir().join("control.sock");
    if !control_sock_path.exists() {
        anyhow::bail!("uscreen daemon is not running (control socket {:?} not found)", control_sock_path);
    }
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut stream = tokio::net::UnixStream::connect(&control_sock_path).await?;
    stream.write_all(format!("{}\n", cmd).as_bytes()).await?;
    let mut reader = tokio::io::BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let resp = line.trim();
    if let Some(err) = resp.strip_prefix("ERR ") {
        anyhow::bail!("Control command failed: {}", err);
    }
    println!("{}", resp);
    Ok(())
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "uscreen=info".into()),
        )
        .with_target(true)
        .with_line_number(true)
        .init();
}

async fn run_daemon(cli: Cli) -> Result<()> {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }

    let pid_path = get_pid_path();
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Refuse to start a second daemon on top of a live one: the PID file is
    // a single slot, so `uscreen stop` only ever kills the most recently
    // started process — any earlier instance still running would become
    // permanently untracked, and both would keep writing/reading the same
    // EVDI FIFO, corrupting frames and starving the encoder.
    if let Ok(existing) = std::fs::read_to_string(&pid_path) {
        if let Ok(existing_pid) = existing.trim().parse::<i32>() {
            let alive = std::path::Path::new(&format!("/proc/{}", existing_pid)).exists();
            if alive {
                anyhow::bail!(
                    "uscreen daemon already running (PID: {}). Run `uscreen stop` first.",
                    existing_pid
                );
            }
        }
    }

    // A daemon that lost its PID file (see remove_pid_file_if_ours) is still
    // a daemon; two of them fight over the EVDI device and the FIFO.
    let others = other_daemons();
    if !others.is_empty() {
        anyhow::bail!(
            "uscreen daemon already running (PID {:?}, untracked). Run `uscreen stop` first.",
            others
        );
    }

    // Write PID file for clean stop/status
    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())?;

    // Settings precedence: CLI flag > config file > built-in default
    let file_cfg = config::FileConfig::load();

    // `load()` clamps unusable values, but leaving the bad number on disk means
    // the GUI keeps showing it and writes it straight back. Heal the file once,
    // here, so every tool agrees on what the settings actually are.
    {
        let raw: Option<config::FileConfig> = std::fs::read_to_string(config::config_path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok());
        if raw.is_some_and(|r| r != file_cfg) {
            match file_cfg.save() {
                Ok(_) => info!("Rewrote out-of-range settings in {:?}", config::config_path()),
                Err(e) => warn!("Could not rewrite the config file: {}", e),
            }
        }
    }
    // Trust the configured encoder only once it has encoded a frame. A fresh
    // config says h264_nvenc, and on a machine without NVIDIA that used to
    // mean ffmpeg dying on every start while the tablet showed a spinner
    // forever (#15). A CLI flag is an explicit request and is left alone.
    let encoder = match cli.encoder.clone() {
        Some(e) => e,
        None => match encoders::resolve(&file_cfg.encoder).await {
            Some(e) => {
                if e != file_cfg.encoder {
                    let mut fixed = file_cfg.clone();
                    fixed.encoder = e.clone();
                    match fixed.save() {
                        Ok(_) => info!("Saved encoder = {} to the config", e),
                        Err(err) => warn!("Could not save the encoder choice: {}", err),
                    }
                }
                e
            }
            None => anyhow::bail!(
                "No working H.264 encoder: ffmpeg could not encode a frame with any of {}. \
                 On Fedora, ffmpeg from RPM Fusion is needed; elsewhere check that ffmpeg \
                 is the full build.",
                encoders::CANDIDATES.join(", ")
            ),
        },
    };
    // Settle the VAAPI render node now, also for an encoder given on the
    // command line, which skips the probe that would otherwise do it.
    if encoder.contains("vaapi") {
        encoders::ensure_vaapi_device(&file_cfg.vaapi_device).await;
    }
    let fps = cli.fps.unwrap_or(file_cfg.fps);
    let bitrate = cli.bitrate.unwrap_or(file_cfg.bitrate);
    let width = cli.width.unwrap_or(file_cfg.width);
    let height = cli.height.unwrap_or(file_cfg.height);
    let video_port = cli.video_port.unwrap_or(file_cfg.video_port);
    let input_port = cli.input_port.unwrap_or(file_cfg.input_port);
    let quality = cli.quality.unwrap_or(file_cfg.quality);
    let stream_scale = cli.stream_scale.unwrap_or(file_cfg.stream_scale);
    let pen_only = cli.pen_only || file_cfg.pen_only;
    let max_performance_mode = cli.max_performance || file_cfg.max_performance_mode;

    let cap_config = capture::CaptureConfig {
        helper_path: find_helper(&cli.helper),
        edid_path: cli.edid.clone(),
        encoder: encoder.clone(),
        fps,
        bitrate,
        width,
        height,
        quality,
        // Replaced as soon as the tablet reports its real panel size.
        width_mm: edid::DEFAULT_WIDTH_MM,
        height_mm: edid::DEFAULT_HEIGHT_MM,
        stream_scale,
        position: config::Position::parse_or_default(&file_cfg.position),
        ten_bit: file_cfg.ten_bit,
        instance: 0,
        // Every helper is pinned to its own card by slot instance.
        card: vdisplay::evdi_cards().first().copied(),
        max_performance_mode,
        hevc_low_latency_tuning: file_cfg.hevc_low_latency_tuning,
        keep_alive: file_cfg.keep_alive,
    };

    // One secret per daemon run. Handed to the app over adb when it is
    // launched; anything connecting to the loopback ports without it gets
    // nothing. See config::FileConfig::require_token.
    let token: Option<String> = if file_cfg.require_token {
        // Fail closed. Running without a token because the runtime dir was
        // unwritable would quietly turn a required check into no check.
        match runtime::new_session_token() {
            Ok(t) => Some(t),
            Err(e) => anyhow::bail!(
                "require_token is on but no session token could be created: {}. \
                 Fix the runtime directory, or set require_token = false.",
                e
            ),
        }
    } else {
        warn!("require_token = false: any local process can read the screen and inject input");
        None
    };
    let relaunch = std::sync::Arc::new(tokio::sync::Notify::new());

    let stream_config = stream::StreamConfig { video_port, token: token.clone() };

    let input_config = input::InputConfig {
        port: input_port,
        instance: 0,
        token: token.clone(),
        codec: capture::Codec::from_encoder(&encoder).muxer().to_string(),
        virtual_width: width,
        virtual_height: height,
        pointer_handoff: file_cfg.pointer_handoff,
    };

    // Which of the two jobs the tablet is doing. Switchable at runtime from
    // the tablet's own settings, so it lives in a channel the input server,
    // the capture manager and the config writer all follow.
    let (mode_tx, _) = watch::channel(pen_only);

    // Live-tunable settings (from the tablet app or by editing the config
    // file). A change restarts the encoder on the fly.
    let (settings_tx, settings_rx) = watch::channel(capture::EncoderSettings {
        encoder: encoder.clone(),
        fps,
        bitrate,
        width,
        height,
        quality,
        width_mm: edid::DEFAULT_WIDTH_MM,
        height_mm: edid::DEFAULT_HEIGHT_MM,
        stream_scale,
        ten_bit: file_cfg.ten_bit,
        keep_alive: file_cfg.keep_alive,
    });

    let mut capture_mgr = capture::CaptureManager::new(cap_config);
    let card_rx = capture_mgr.card_rx();
    let codec_config = capture_mgr.codec_config_arc();
    let latency = capture_mgr.latency_tracker();
    let stream_srv =
        stream::StreamServer::new(stream_config, codec_config, capture_mgr.idr_request_flag());
    let input_srv = input::InputServer::new(
        input_config,
        Some(settings_tx.clone()),
        mode_tx.clone(),
        latency,
        relaunch.clone(),
        card_rx.clone(),
    );

    // Deliberately shallow. This ring is pure latency when it fills: 256 frames
    // is four seconds of backlog at 60 fps, and a client that fell behind would
    // dutifully receive all of it instead of skipping to something current.
    // At 8, `RecvError::Lagged` fires early and the skip-to-newest-IDR path in
    // the stream server actually gets a chance to run.
    let (video_tx, _) = broadcast::channel(8);

    // The tablet is a touchscreen as far as the desktop is concerned, so the
    // virtual keyboard would pop up over the screen being used as a monitor.
    osk::disable().await;

    info!("=== uscreen daemon starting ===");
    info!("  Resolution: {}x{} @ {}fps", width, height, fps);
    info!("  Encoder: {}", encoder);
    info!("  Bitrate: {} kbps", bitrate);
    info!("  Stream port: {}", video_port);
    info!("  Input port: {}", input_port);

    // Tablet presence, published by the ADB monitor.
    let (tablet_tx, mut tablet_rx) = watch::channel(false);
    let tray_tablet_rx = tablet_tx.subscribe();

    // What the capture manager actually follows: the virtual display should
    // exist exactly while a tablet is attached *and* being used as a screen.
    // Folding the mode in here is what makes switching live — the capture
    // manager already knows how to raise and tear down the display on this
    // signal, and cannot tell the difference between an unplugged tablet and
    // one that is currently a drawing surface.
    let (gate_tx, gate_rx) = watch::channel(false);
    let mut mode_rx_gate = mode_tx.subscribe();
    tokio::spawn(async move {
        let mut last = false;
        loop {
            let active = *tablet_rx.borrow() && !*mode_rx_gate.borrow();
            if active != last {
                last = active;
                let _ = gate_tx.send(active);
            }
            tokio::select! {
                r = tablet_rx.changed() => if r.is_err() { break },
                r = mode_rx_gate.changed() => if r.is_err() { break },
            }
        }
    });

    // Cooperative shutdown: the capture task must get a chance to kill and reap
    // ffmpeg/evdi_helper before the process exits, or they linger holding the
    // capture FIFO and collide with the next start.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    if pen_only {
        info!("Starting in pen-only mode: the tablet drives this machine's own");
        info!("  screen with the pen. No virtual display, no encoding.");
    }

    let video_tx_cap = video_tx.clone();
    let settings_rx_cap = settings_rx.clone();
    let cap_handle = tokio::spawn(async move {
        // Started in both modes. In pen-only the gate below holds the virtual
        // output disabled and nothing is encoded, but the helper is up and
        // ready — which is what lets a switch back to second-screen mode show
        // a picture immediately instead of rebuilding the pipeline first.
        if let Err(e) = capture_mgr
            .stream_frames(video_tx_cap, settings_rx_cap, gate_rx, shutdown_rx)
            .await
        {
            error!("Capture manager failed: {}", e);
        }
    });

    let video_rx = video_tx.subscribe();
    let stream_handle = tokio::spawn(async move {
        if let Err(e) = stream_srv.run(video_rx).await {
            error!("Stream server failed: {}", e);
        }
    });

    let input_handle = tokio::spawn(async move {
        if let Err(e) = input_srv.run().await {
            error!("Input server failed: {}", e);
        }
    });

    // Fields the user overrode on the command line for this run only. They
    // must not be written back: a flag is not a settings change, and
    // persisting one silently rewrites the user's configuration behind them.
    let cli_overrides = (
        cli.encoder.is_some(),
        cli.fps.is_some(),
        cli.bitrate.is_some(),
        cli.width.is_some(),
        cli.height.is_some(),
        cli.quality.is_some(),
        cli.stream_scale.is_some(),
    );

    // Which tablet the settings being saved belong to, so they can be
    // remembered per device as well as globally.
    let (serial_tx, serial_rx) = watch::channel(None::<String>);

    // Synchronize app locking with GNOME Shell extension
    let mut card_rx_applock = card_rx.clone();
    let mut serial_rx_applock = serial_rx.clone();
    tokio::spawn(async move {
        loop {
            let card_opt = *card_rx_applock.borrow();
            let conn = card_opt.and_then(|c_idx| {
                vdisplay::evdi_connectors()
                    .into_iter()
                    .find(|c| c.card == c_idx)
                    .map(|c| c.name)
            });
            let serial = serial_rx_applock.borrow().clone();
            sync_app_lock(conn.as_deref(), serial.as_deref());

            tokio::select! {
                r = card_rx_applock.changed() => if r.is_err() { break },
                r = serial_rx_applock.changed() => if r.is_err() { break },
            }
        }
    });

    // Persist settings changes pushed at runtime back to the config file
    let serial_rx_save = serial_rx.clone();
    let mut settings_rx_save = settings_rx.clone();
    let save_handle = tokio::spawn(async move {
        while settings_rx_save.changed().await.is_ok() {
            let s = settings_rx_save.borrow().clone();
            let mut cfg = config::FileConfig::load();
            let (has_enc_override, has_fps_override, has_bitrate_override, has_quality_override, has_scale_override, has_ten_bit_override) =
                if let Some(serial) = serial_rx_save.borrow().as_ref() {
                    if let Some(p) = cfg.tablets.get(serial) {
                        (p.encoder.is_some(), p.fps.is_some(), p.bitrate.is_some(), p.quality.is_some(), p.stream_scale.is_some(), p.ten_bit.is_some())
                    } else {
                        (false, false, false, false, false, false)
                    }
                } else {
                    (false, false, false, false, false, false)
                };

            if !cli_overrides.0 && !has_enc_override { cfg.encoder = s.encoder; }
            if !cli_overrides.1 && !has_fps_override { cfg.fps = s.fps; }
            if !cli_overrides.2 && !has_bitrate_override { cfg.bitrate = s.bitrate; }
            if !cli_overrides.3 { cfg.width = s.width; }
            if !cli_overrides.4 { cfg.height = s.height; }
            if !cli_overrides.5 && !has_quality_override { cfg.quality = s.quality; }
            if !cli_overrides.6 && !has_scale_override { cfg.stream_scale = s.stream_scale; }
            if !has_ten_bit_override { cfg.ten_bit = s.ten_bit; }
            // And against this tablet's serial, so the next session with it
            // builds the right display immediately instead of rebuilding.
            if let Some(serial) = serial_rx_save.borrow().clone() {
                let p = cfg.tablets.entry(serial).or_default();
                if s.width > 0 && s.height > 0 {
                    p.width = s.width;
                    p.height = s.height;
                }
                if s.width_mm > 0 && s.height_mm > 0 {
                    p.width_mm = s.width_mm;
                    p.height_mm = s.height_mm;
                }
            }
            if let Err(e) = cfg.save() {
                warn!("Failed to persist settings: {}", e);
            } else {
                info!("Settings saved to {:?}", config::config_path());
            }
        }
    });

    // Remember which mode the tablet was left in. Unlike the --pen-only flag,
    // which is a one-off for this run and never written back, a switch made
    // from the tablet is a deliberate choice and should survive a restart.
    let mut mode_rx_save = mode_tx.subscribe();
    let mode_save_handle = tokio::spawn(async move {
        while mode_rx_save.changed().await.is_ok() {
            let pen_only = *mode_rx_save.borrow();
            let mut cfg = config::FileConfig::load();
            cfg.pen_only = pen_only;
            if let Err(e) = cfg.save() {
                warn!("Failed to persist mode: {}", e);
            }
        }
    });

    // The daemon's only face on the desktop. It follows the same channels the
    // rest of the daemon does, so it cannot drift out of step with what is
    // actually running.
    // Once a day, ask GitHub whether there is a newer release. Reported in
    // the tray and by doctor; never installed from here.
    let (update_tx, update_rx) = watch::channel::<update::Available>(None);
    let update_handle = if file_cfg.check_updates {
        Some(tokio::spawn(async move { update::run(update_tx).await }))
    } else {
        None
    };

    let tray_mode_tx = mode_tx.clone();
    let tray_shutdown_tx = shutdown_tx.clone();
    let tray_handle = tokio::spawn(async move {
        tray::run(tray_mode_tx, tray_tablet_rx, tray_shutdown_tx, update_rx).await;
    });

    // Plug-and-play: watch for the tablet over ADB, set up port forwarding
    // and launch the app whenever it's (re)connected.
    let auto_launch = file_cfg.auto_launch_app;
    let adb_token = token.clone();
    let extra = ExtraSessionTemplate {
        max_tablets: file_cfg.max_tablets,
        cap_template: capture::CaptureConfig {
            helper_path: find_helper(&cli.helper),
            edid_path: None,
            encoder: encoder.clone(),
            fps,
            bitrate,
            width,
            height,
            quality,
            width_mm: edid::DEFAULT_WIDTH_MM,
            height_mm: edid::DEFAULT_HEIGHT_MM,
            stream_scale,
            position: config::Position::parse_or_default(&file_cfg.position),
            ten_bit: file_cfg.ten_bit,
            instance: 0,
            card: None,
            max_performance_mode,
            hevc_low_latency_tuning: file_cfg.hevc_low_latency_tuning,
            keep_alive: file_cfg.keep_alive,
        },
        video_port,
        input_port,
        token: token.clone(),
        mode_tx: mode_tx.clone(),
        shutdown_rx: shutdown_tx.subscribe(),
    };
    let wifi_address = file_cfg.wifi_address.clone();
    let settings_tx_adb = settings_tx.clone();
    let (control_tx, control_rx) = mpsc::channel::<DaemonControlMsg>(16);
    let adb_handle = tokio::spawn(async move {
        adb_monitor(video_port, input_port, auto_launch, tablet_tx, adb_token, relaunch, extra, wifi_address, settings_tx_adb, serial_tx, control_rx).await;
    });

    let control_sock_path = crate::runtime::runtime_dir().join("control.sock");
    let _ = std::fs::remove_file(&control_sock_path);
    let control_handle = match tokio::net::UnixListener::bind(&control_sock_path) {
        Ok(listener) => {
            let ctrl_tx = control_tx.clone();
            Some(tokio::spawn(async move {
                run_control_server(listener, ctrl_tx).await;
            }))
        }
        Err(e) => {
            warn!("Could not bind control.sock: {}", e);
            None
        }
    };

    println!();
    println!("================================================");
    println!("  uscreen daemon running (PID: {})", pid);
    println!("================================================");
    println!("  On your tablet, open the UScreen app");
    println!("  ADB ports will be auto-forwarded if possible.");
    println!("  Otherwise, run:");
    println!("    adb reverse tcp:8890 tcp:8890");
    println!("    adb reverse tcp:8891 tcp:8891");
    println!("================================================");
    println!();

    // `uscreen stop` (and the GUI) send SIGTERM, not SIGINT — without a
    // handler for it, the kernel kills the process with its default
    // disposition and none of our cleanup (which is what kills the
    // evdi_helper/ffmpeg children via kill_on_drop) ever runs, orphaning
    // them to fight over the shared FIFO with the next daemon that starts.
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    // Quit from the tray raises the same flag the signal handlers do, so it
    // has to be waited on here too — otherwise the pipeline winds down while
    // the process itself stays alive with nothing left to run.
    let mut quit_rx = shutdown_tx.subscribe();
    tokio::select! {
        _ = signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
        _ = async { while quit_rx.changed().await.is_ok() && !*quit_rx.borrow() {} } => {}
    }
    info!("Shutting down...");

    // Ask the capture pipeline to wind down, and give it a bounded moment to
    // actually reap its children before pulling the rug out.
    let _ = shutdown_tx.send(true);
    if tokio::time::timeout(std::time::Duration::from_secs(5), cap_handle)
        .await
        .is_err()
    {
        warn!("Capture pipeline did not stop within 5s");
    }

    osk::restore().await;

    stream_handle.abort();
    input_handle.abort();
    adb_handle.abort();
    if let Some(h) = control_handle {
        h.abort();
    }
    let _ = std::fs::remove_file(&control_sock_path);
    save_handle.abort();
    mode_save_handle.abort();
    tray_handle.abort();
    if let Some(h) = update_handle {
        h.abort();
    }

    // Clean up PID file
    // Only if it is still ours. Winding down takes a few seconds, and a
    // daemon started in the meantime has already written its own PID here;
    // deleting that leaves the new daemon untracked - doctor calls it an
    // orphan and `uscreen stop` can no longer find it.
    remove_pid_file_if_ours(&pid_path, pid);

    info!("uscreen daemon stopped");
    Ok(())
}

/// PIDs of every other `uscreen start` process of this user, found by
/// process name plus argument rather than by grepping full command lines -
/// `pgrep -f 'uscreen start'` also matched the shells running it, which is
/// how `uscreen stop` used to kill the terminal it was typed into.
fn other_daemons() -> Vec<u32> {
    use std::os::unix::fs::MetadataExt;
    let me = std::process::id();
    let ppid = unsafe { libc::getppid() as u32 };
    let uid = unsafe { libc::getuid() };
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return out };
    for e in dir.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        if pid == me || pid == ppid { continue; }
        let base = e.path();
        let Ok(comm) = std::fs::read_to_string(base.join("comm")) else { continue };
        if comm.trim() != "uscreen" { continue; }
        let Ok(cmd) = std::fs::read(base.join("cmdline")) else { continue };
        // A daemon is `uscreen start` — or plain `uscreen`, since no
        // subcommand means start. Only the other subcommands are not daemons.
        let args: Vec<&[u8]> = cmd.split(|b| *b == 0).skip(1).collect();
        let other_subcommand = args.iter().any(|a| {
            matches!(*a, b"stop" | b"status" | b"list-displays" | b"doctor" | b"wifi" | b"--help" | b"-h" | b"--version" | b"-V" | b"-d" | b"--daemonize" | b"pause-device" | b"resume-device")
        });
        if other_subcommand { continue; }
        // Same user only: another account's daemon is not ours to touch.
        if std::fs::metadata(&base).map(|m| m.uid()).unwrap_or(u32::MAX) != uid { continue; }
        out.push(pid);
    }
    out
}

async fn run_control_server(
    listener: tokio::net::UnixListener,
    control_tx: mpsc::Sender<DaemonControlMsg>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    loop {
        let (mut socket, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let ctrl_tx = control_tx.clone();
        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut buf_reader = tokio::io::BufReader::new(reader);
            let mut line = String::new();
            while let Ok(n) = buf_reader.read_line(&mut line).await {
                if n == 0 { break; }
                let cmd = line.trim();
                if let Some(serial) = cmd.strip_prefix("PAUSE ") {
                    let serial = serial.trim().to_string();
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx.send(DaemonControlMsg::PauseDevice { serial: serial.clone(), resp: tx }).await;
                    match rx.await {
                        Ok(Ok(())) => {
                            let _ = writer.write_all(format!("OK PAUSED {}\n", serial).as_bytes()).await;
                        }
                        Ok(Err(e)) => {
                            let _ = writer.write_all(format!("ERR {}\n", e).as_bytes()).await;
                        }
                        Err(_) => {
                            let _ = writer.write_all(b"ERR channel closed\n").await;
                        }
                    }
                } else if let Some(serial) = cmd.strip_prefix("RESUME ") {
                    let serial = serial.trim().to_string();
                    let (tx, rx) = oneshot::channel();
                    let _ = ctrl_tx.send(DaemonControlMsg::ResumeDevice { serial: serial.clone(), resp: tx }).await;
                    match rx.await {
                        Ok(Ok(())) => {
                            let _ = writer.write_all(format!("OK RESUMED {}\n", serial).as_bytes()).await;
                        }
                        Ok(Err(e)) => {
                            let _ = writer.write_all(format!("ERR {}\n", e).as_bytes()).await;
                        }
                        Err(_) => {
                            let _ = writer.write_all(b"ERR channel closed\n").await;
                        }
                    }
                } else if cmd == "PING" {
                    let _ = writer.write_all(b"PONG\n").await;
                } else {
                    let _ = writer.write_all(b"ERR unknown command\n").await;
                }
                let _ = writer.flush().await;
                line.clear();
            }
        });
    }
}

fn remove_pid_file_if_ours(pid_path: &std::path::Path, pid: u32) {
    let ours = std::fs::read_to_string(pid_path)
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
        .map(|p| p == pid)
        .unwrap_or(false);
    if ours {
        let _ = std::fs::remove_file(pid_path);
    }
}

fn get_pid_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{}/.local/share/uscreen/uscreen.pid", home))
}

/// Resolve the helper binary.
///
/// An explicitly given `--helper` always wins. It used to lose to the copy in
/// `~/.local/bin`, so a freshly built helper was silently ignored in favour of
/// whatever was last installed — the kind of thing that costs an afternoon.
fn find_helper(path: &std::path::Path) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();

    if path.exists() {
        if let Ok(canon) = path.canonicalize() {
            return canon;
        }
        return path.to_path_buf();
    }

    // The helper that belongs to *this* install first, derived from where
    // this binary lives: next to it (install.sh puts both in ~/.local/bin),
    // or ../lib/uscreen and ../lib64/uscreen (packages put uscreen in
    // /usr/bin and the helper under /usr/lib*/uscreen). Only then the
    // generic locations. A packaged /usr/bin/uscreen must never pick up a
    // stale ~/.local/bin/evdi_helper left by an earlier install.sh - one
    // built against the old, too-short evdi header, say.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("evdi_helper"));
            if let Some(prefix) = dir.parent() {
                candidates.push(prefix.join("lib/uscreen/evdi_helper"));
                candidates.push(prefix.join("lib64/uscreen/evdi_helper"));
                candidates.push(prefix.join("libexec/uscreen/evdi_helper"));
            }
        }
    }
    candidates.extend(
        [
            format!("{}/.local/bin/evdi_helper", home),
            "/usr/lib/uscreen/evdi_helper".to_string(),
            "/usr/lib64/uscreen/evdi_helper".to_string(),
            "/usr/libexec/uscreen/evdi_helper".to_string(),
            "/usr/local/lib/uscreen/evdi_helper".to_string(),
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    for p in candidates {
        if p.exists() {
            return p;
        }
    }

    let alt = PathBuf::from("host/evdi/evdi_helper");
    if alt.exists() {
        if let Ok(canon) = alt.canonicalize() {
            return canon;
        }
        return alt;
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        for _ in 0..5 {
            if let Some(d) = dir {
                let from_exe = d.join("host").join("evdi").join("evdi_helper");
                if from_exe.exists() {
                    if let Ok(canon) = from_exe.canonicalize() {
                        return canon;
                    }
                    return from_exe;
                }
                dir = d.parent();
            } else {
                break;
            }
        }
    }

    path.to_path_buf()
}

/// Keeps watching for the tablet. On every (re)connect: set up reverse port
/// forwarding and optionally launch the UScreen app — plug in and it works.
///
/// Presence is published on `tablet_tx` so the capture manager can bring the
/// virtual display up and down along with the tablet.
/// Everything needed to bring up a pipeline for a second (third, ...)
/// tablet on demand. The first tablet is wired at startup like it always
/// was; these are spawned when another serial shows up and torn down when
/// it goes.
struct ExtraSessionTemplate {
    max_tablets: u32,
    cap_template: capture::CaptureConfig,
    video_port: u16,
    input_port: u16,
    token: Option<String>,
    mode_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

/// One extra tablet's running pipeline.
struct ExtraSession {
    instance: u32,
    #[allow(dead_code)]
    hw_id: String,
    active_serial: String,
    transport: Transport,
    /// This tablet's own settings, so its remembered panel can be applied to
    /// it and whatever it reports can be written back under its serial.
    settings_tx: watch::Sender<capture::EncoderSettings>,
    tablet_tx: watch::Sender<bool>,
    relaunch: std::sync::Arc<tokio::sync::Notify>,
    stop_tx: watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    video_port: u16,
    input_port: u16,
    last_relaunch: std::time::Instant,
    relaunch_wait: std::time::Duration,
    relaunches: u32,
}

impl ExtraSession {
    fn stop_background(self) {
        tokio::spawn(async move {
            let _ = self.tablet_tx.send(false);
            let _ = self.stop_tx.send(true);
            // Let the capture manager disable its output and reap its children
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            for t in self.tasks {
                t.abort();
            }
        });
    }
}

/// Bring up capture, stream and input for tablet number `instance`.
/// Ports are the base ports plus 2 per instance; the tablet side keeps
/// using 8890/8891, since `adb reverse` maps them per device.
fn spawn_extra_session(
    t: &ExtraSessionTemplate,
    instance: u32,
    serial: &str,
    hw_id: String,
    transport: Transport,
) -> Option<ExtraSession> {
    let cards = vdisplay::evdi_cards();
    let mut cfg = t.cap_template.clone();
    cfg.instance = instance;
    // Check if this device has per-device overrides configured
    let file_cfg = config::FileConfig::load();
    if let Some(p) = file_cfg.tablets.get(serial) {
        if let Some(ref enc) = p.encoder {
            if !enc.trim().is_empty() {
                cfg.encoder = enc.clone();
            }
        }
        if let Some(fps) = p.fps {
            cfg.fps = fps;
        }
        if let Some(bitrate) = p.bitrate {
            cfg.bitrate = bitrate;
        }
        if let Some(quality) = p.quality {
            cfg.quality = quality;
        }
        if let Some(stream_scale) = p.stream_scale {
            cfg.stream_scale = stream_scale;
        }
        if let Some(ten_bit) = p.ten_bit {
            cfg.ten_bit = ten_bit;
        }
        if let Some(keep_alive) = p.keep_alive {
            cfg.keep_alive = keep_alive;
        }
        if let Some(ref pos) = p.position {
            cfg.position = config::Position::parse_or_default(pos);
        }
        if p.has_display() {
            cfg.width = p.width;
            cfg.height = p.height;
            if p.width_mm > 0 && p.height_mm > 0 {
                cfg.width_mm = p.width_mm;
                cfg.height_mm = p.height_mm;
            }
        }
    }
    let codec = capture::Codec::from_encoder(&cfg.encoder).muxer().to_string();
    // Each tablet owns one EVDI card, by slot. Starting a session without one
    // used to leave its helper to find a free card by itself, which with
    // several starting at once means two helpers on the same card and two
    // streams of the same screen. Better to say what is missing and stop.
    cfg.card = cards.get(instance as usize).copied();
    if cfg.card.is_none() {
        warn!(
            "Tablet {} has no EVDI device of its own: {} exist, {} are wanted. \
             Put initial_device_count={} in /etc/modprobe.d/uscreen-evdi.conf, \
             then `sudo modprobe -r evdi && sudo modprobe evdi` or reboot.",
            instance + 1,
            cards.len(),
            t.max_tablets,
            t.max_tablets
        );
        return None;
    }
    let video_port = t.video_port + 2 * instance as u16;
    let input_port = t.input_port + 2 * instance as u16;

    let (settings_tx, settings_rx) = watch::channel(capture::EncoderSettings {
        encoder: cfg.encoder.clone(),
        fps: cfg.fps,
        bitrate: cfg.bitrate,
        width: cfg.width,
        height: cfg.height,
        quality: cfg.quality,
        width_mm: cfg.width_mm,
        height_mm: cfg.height_mm,
        stream_scale: cfg.stream_scale,
        ten_bit: cfg.ten_bit,
        keep_alive: cfg.keep_alive,
    });
    let settings_tx_keep = settings_tx.clone();
    let mut cap = capture::CaptureManager::new(cfg.clone());
    let card_rx = cap.card_rx();
    let codec_config = cap.codec_config_arc();
    let latency = cap.latency_tracker();
    let relaunch = std::sync::Arc::new(tokio::sync::Notify::new());
    let stream_srv = stream::StreamServer::new(
        stream::StreamConfig { video_port, token: t.token.clone() },
        codec_config,
        cap.idr_request_flag(),
    );
    let input_srv = input::InputServer::new(
        input::InputConfig {
            port: input_port,
            instance,
            token: t.token.clone(),
            codec,
            virtual_width: cfg.width,
            virtual_height: cfg.height,
            pointer_handoff: config::FileConfig::load().pointer_handoff,
        },
        Some(settings_tx),
        t.mode_tx.clone(),
        latency,
        relaunch.clone(),
        card_rx.clone(),
    );

    let (tablet_tx, mut tablet_rx) = watch::channel(false);
    let (gate_tx, gate_rx) = watch::channel(false);
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut mode_rx = t.mode_tx.subscribe();
    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        let mut last = false;
        loop {
            let active = *tablet_rx.borrow() && !*mode_rx.borrow();
            if active != last {
                last = active;
                let _ = gate_tx.send(active);
            }
            tokio::select! {
                r = tablet_rx.changed() => if r.is_err() { break },
                r = mode_rx.changed() => if r.is_err() { break },
            }
        }
    }));
    let (video_tx, _) = broadcast::channel(8);
    let video_rx = video_tx.subscribe();
    // Either the whole daemon stopping or this session being torn down
    // must wind the capture pipeline down cleanly.
    let mut daemon_stop = t.shutdown_rx.clone();
    let (cap_stop_tx, cap_stop_rx) = watch::channel(false);
    let mut stop_rx_c = stop_rx.clone();
    tasks.push(tokio::spawn(async move {
        tokio::select! {
            _ = daemon_stop.changed() => {}
            _ = stop_rx_c.changed() => {}
        }
        let _ = cap_stop_tx.send(true);
    }));
    tasks.push(tokio::spawn(async move {
        if let Err(e) = cap.stream_frames(video_tx, settings_rx, gate_rx, cap_stop_rx).await {
            error!("Capture manager {} failed: {}", instance, e);
        }
    }));
    tasks.push(tokio::spawn(async move {
        if let Err(e) = stream_srv.run(video_rx).await {
            error!("Stream server {} failed: {}", instance, e);
        }
    }));
    tasks.push(tokio::spawn(async move {
        if let Err(e) = input_srv.run().await {
            error!("Input server {} failed: {}", instance, e);
        }
    }));
    info!(
        "Tablet slot {} ready: video port {}, input port {}{}",
        instance + 1,
        video_port,
        input_port,
        cfg.card.map(|c| format!(", EVDI card{}", c)).unwrap_or_default()
    );
    Some(ExtraSession {
        instance,
        hw_id,
        active_serial: serial.to_string(),
        transport,
        tablet_tx,
        relaunch,
        stop_tx,
        tasks,
        video_port,
        input_port,
        settings_tx: settings_tx_keep,
        last_relaunch: std::time::Instant::now() - std::time::Duration::from_secs(60),
        relaunch_wait: std::time::Duration::from_secs(5),
        relaunches: 0,
    })
}

#[derive(Clone, Debug, PartialEq)]
struct DiscoveredDevice {
    serial: String,
    state: String,
    transport: Transport,
    hw_id: String,
}

async fn resolve_hw_id(
    serial: &str,
    cache: &mut std::collections::HashMap<String, String>,
) -> String {
    if !is_network_serial(serial) {
        return serial.to_string();
    }
    if let Some(cached) = cache.get(serial) {
        return cached.clone();
    }
    let file_cfg = config::FileConfig::load();
    for (k, v) in &file_cfg.tablets {
        if v.wifi_address.as_deref() == Some(serial) {
            cache.insert(serial.to_string(), k.clone());
            return k.clone();
        }
    }
    if let Ok(o) = tokio::time::timeout(
        std::time::Duration::from_millis(800),
        tokio::process::Command::new("adb")
            .args(["-s", serial, "shell", "getprop", "ro.serialno"])
            .output(),
    )
    .await
    {
        if let Ok(output) = o {
            let hw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !hw.is_empty() && hw != "unknown" {
                cache.insert(serial.to_string(), hw.clone());
                return hw;
            }
        }
    }
    serial.to_string()
}

async fn scan_adb_devices(
    hw_cache: &mut std::collections::HashMap<String, String>,
) -> Vec<DiscoveredDevice> {
    let Ok(out) = tokio::process::Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .await
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut out_devices = Vec::new();

    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let Some(state) = parts.next() else { continue };
        let transport = transport_of(serial);
        let hw_id = if transport == Transport::Usb {
            serial.to_string()
        } else {
            resolve_hw_id(serial, hw_cache).await
        };
        out_devices.push(DiscoveredDevice {
            serial: serial.to_string(),
            state: state.to_string(),
            transport,
            hw_id,
        });
    }

    // Support USCREEN_FAKE_TABLET test hook
    if let Ok(fake) = std::env::var("USCREEN_FAKE_TABLET") {
        for f in fake.split(',').map(str::trim).filter(|f| !f.is_empty()) {
            if !out_devices.iter().any(|d| d.serial == f) {
                out_devices.push(DiscoveredDevice {
                    serial: f.to_string(),
                    state: "device".to_string(),
                    transport: transport_of(f),
                    hw_id: f.to_string(),
                });
            }
        }
    }


    out_devices
}

/// Keeps watching for tablets. The first serial seen gets the pipeline wired
/// at startup; with max_tablets > 1, further serials each get a pipeline of
/// their own for as long as they stay attached.
///
/// Presence is published on `tablet_tx` so the capture manager can bring the
/// virtual display up and down along with the tablet.
#[allow(clippy::too_many_arguments)]
async fn adb_monitor(
    video_port: u16,
    input_port: u16,
    auto_launch: bool,
    tablet_tx: watch::Sender<bool>,
    token: Option<String>,
    relaunch: std::sync::Arc<tokio::sync::Notify>,
    extra: ExtraSessionTemplate,
    // `ip:port` remembered by `uscreen wifi`, or empty.
    wifi_address: String,
    settings_tx: watch::Sender<capture::EncoderSettings>,
    serial_tx: watch::Sender<Option<String>>,
    mut control_rx: mpsc::Receiver<DaemonControlMsg>,
) {
    let mut paused_serials: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut slot_0_hw: Option<String> = None;
    let mut slot_0_active_serial: String = String::new();
    let mut slot_0_transport: Transport = Transport::Usb;
    let mut slot_0_last_relaunch = std::time::Instant::now() - std::time::Duration::from_secs(60);
    let mut slot_0_relaunches: u32 = 0;
    let mut slot_0_relaunch_wait = std::time::Duration::from_secs(5);
    const RELAUNCH_WAIT_MAX: std::time::Duration = std::time::Duration::from_secs(600);

    let mut polls_since_check: u32 = 0;
    const APP_CHECK_EVERY: u32 = 2; // Check watchdog every 4s
    let mut extras: std::collections::HashMap<String, ExtraSession> = std::collections::HashMap::new();
    let mut hw_cache: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut offline_since: std::collections::HashMap<String, std::time::Instant> = std::collections::HashMap::new();

    if tokio::process::Command::new("adb").arg("version").output().await.is_err() {
        error!("adb is not installed — devices cannot be found. Install android-tools (or adb) and restart.");
    }

    loop {
        let all_devices = scan_adb_devices(&mut hw_cache).await;

        // ADB State Hygiene: handle offline devices with targeted reconnect without killing adb server
        for dev in &all_devices {
            if dev.state == "offline" {
                let entry = offline_since.entry(dev.serial.clone()).or_insert_with(std::time::Instant::now);
                if entry.elapsed() >= std::time::Duration::from_secs(4) {
                    warn!("Device {} is offline ({:?}) — issuing targeted reconnect", dev.serial, entry.elapsed());
                    if dev.transport == Transport::Network {
                        let _ = tokio::process::Command::new("adb").args(["disconnect", &dev.serial]).output().await;
                    } else {
                        let _ = tokio::process::Command::new("adb").args(["-s", &dev.serial, "reconnect"]).output().await;
                    }
                    *entry = std::time::Instant::now();
                }
            } else {
                offline_since.remove(&dev.serial);
            }
        }

        // Group ready devices by unique physical hardware ID, preferring USB over Network
        let ready_devices: Vec<&DiscoveredDevice> = all_devices.iter().filter(|d| d.state == "device").collect();
        let mut physical_devices: std::collections::HashMap<String, &DiscoveredDevice> = std::collections::HashMap::new();
        for dev in ready_devices {
            physical_devices
                .entry(dev.hw_id.clone())
                .and_modify(|existing| {
                    if existing.transport != Transport::Usb && dev.transport == Transport::Usb {
                        *existing = dev;
                    }
                })
                .or_insert(dev);
        }

        // 1. Maintain Slot 0 Session with strict device isolation
        if let Some(ref s0_hw) = slot_0_hw.clone() {
            if paused_serials.contains(&slot_0_active_serial) {
                // Device is paused for surgical update — do not alter transport or state
            } else if let Some(best_dev) = physical_devices.get(s0_hw) {
                if !*tablet_tx.borrow() {
                    let _ = tablet_tx.send(true);
                }
                // Device is active. Check for seamless USB <-> Wi-Fi handoff for this physical device
                if best_dev.serial != slot_0_active_serial {
                    info!(
                        "[TRANSPORT-SWITCH] Seamless handoff: Slot 1 switched transport from {} ({}) to {} ({}) without display teardown",
                        slot_0_transport.label(), slot_0_active_serial,
                        best_dev.transport.label(), best_dev.serial
                    );
                    if !is_fake_serial(&slot_0_active_serial) {
                        teardown_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                    }
                    if !is_fake_serial(&best_dev.serial) {
                        on_tablet_connected(&best_dev.serial, video_port, input_port, auto_launch, token.as_deref()).await;
                        if best_dev.transport == Transport::Usb {
                            auto_provision_wifi(&best_dev.serial).await;
                        }
                    }
                    slot_0_active_serial = best_dev.serial.clone();
                    slot_0_transport = best_dev.transport;
                    let _ = serial_tx.send(Some(best_dev.serial.clone()));
                    announce_transport(&best_dev.serial);
                }
            } else {
                // Device s0_hw has disconnected
                info!("Device on Slot 1 disconnected ({})", slot_0_active_serial);
                if !is_fake_serial(&slot_0_active_serial) {
                    teardown_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                }
                let _ = tablet_tx.send(false);
                let _ = serial_tx.send(None);
                slot_0_hw = None;
                slot_0_active_serial.clear();
            }
        }

        // 2. Maintain Extra Sessions with strict device isolation
        let mut gone_extras = Vec::new();
        for (hw_id, sess) in extras.iter_mut() {
            if paused_serials.contains(&sess.active_serial) {
                // Device is paused for surgical update — do not touch
                continue;
            }
            if let Some(best_dev) = physical_devices.get(hw_id) {
                if !*sess.tablet_tx.borrow() {
                    let _ = sess.tablet_tx.send(true);
                }
                if best_dev.serial != sess.active_serial {
                    info!(
                        "[TRANSPORT-SWITCH] Seamless handoff: Slot {} switched transport from {} ({}) to {} ({}) without display teardown",
                        sess.instance + 1,
                        sess.transport.label(), sess.active_serial,
                        best_dev.transport.label(), best_dev.serial
                    );
                    if !is_fake_serial(&sess.active_serial) {
                        teardown_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                    }
                    if !is_fake_serial(&best_dev.serial) {
                        on_tablet_connected(&best_dev.serial, sess.video_port, sess.input_port, auto_launch, token.as_deref()).await;
                        if best_dev.transport == Transport::Usb {
                            auto_provision_wifi(&best_dev.serial).await;
                        }
                    }
                    sess.active_serial = best_dev.serial.clone();
                    sess.transport = best_dev.transport;
                    announce_transport(&best_dev.serial);
                }
            } else {
                gone_extras.push(hw_id.clone());
            }
        }

        for hw_id in gone_extras {
            if let Some(sess) = extras.remove(&hw_id) {
                info!("Device on Slot {} disconnected ({})", sess.instance + 1, sess.active_serial);
                if !is_fake_serial(&sess.active_serial) {
                    teardown_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                }
                sess.stop_background();
            }
        }

        // 3. Assign newly attached devices to free slots without disturbing active sessions
        for (hw_id, dev) in &physical_devices {
            if paused_serials.contains(&dev.serial) {
                // Device is in surgical update pause — do not assign as new session
                continue;
            }
            if slot_0_hw.as_ref() == Some(hw_id) || extras.contains_key(hw_id) {
                // Device already has an active session — leave it alone
                continue;
            }

            if slot_0_hw.is_none() {
                info!(
                    "Device connected on Slot 1 over {} ({})",
                    dev.transport.label(),
                    dev.serial
                );
                announce_transport(&dev.serial);
                let _ = serial_tx.send(Some(dev.serial.clone()));
                apply_tablet_profile(&dev.serial, &settings_tx).await;
                if !is_fake_serial(&dev.serial) {
                    on_tablet_connected(&dev.serial, video_port, input_port, auto_launch, token.as_deref()).await;
                    if dev.transport == Transport::Usb {
                        auto_provision_wifi(&dev.serial).await;
                    }
                }
                let _ = tablet_tx.send(true);
                slot_0_hw = Some(hw_id.clone());
                slot_0_active_serial = dev.serial.clone();
                slot_0_transport = dev.transport;
                slot_0_relaunches = 0;
                slot_0_relaunch_wait = std::time::Duration::from_secs(5);
            } else if extra.max_tablets > 1 {
                let used: Vec<u32> = extras.values().map(|s| s.instance).collect();
                let Some(instance) = (1..extra.max_tablets).find(|i| !used.contains(i)) else {
                    warn!("Device {} attached but all {} slots are taken", dev.serial, extra.max_tablets);
                    continue;
                };
                info!("Device connected on Slot {} over {} ({})", instance + 1, dev.transport.label(), dev.serial);
                let Some(sess) = spawn_extra_session(&extra, instance, &dev.serial, hw_id.clone(), dev.transport) else {
                    continue;
                };
                apply_tablet_profile(&dev.serial, &sess.settings_tx).await;
                {
                    let mut rx = sess.settings_tx.subscribe();
                    let serial_for_save = dev.serial.clone();
                    tokio::spawn(async move {
                        while rx.changed().await.is_ok() {
                            let s = rx.borrow().clone();
                            if s.width == 0 || s.height == 0 {
                                continue;
                            }
                            let mut cfg = config::FileConfig::load();
                            let p = cfg.tablets.entry(serial_for_save.clone()).or_default();
                            let before = p.clone();
                            p.width = s.width;
                            p.height = s.height;
                            if s.width_mm > 0 && s.height_mm > 0 {
                                p.width_mm = s.width_mm;
                                p.height_mm = s.height_mm;
                            }
                            if *p != before {
                                let _ = cfg.save();
                            }
                        }
                    });
                }
                if !is_fake_serial(&dev.serial) {
                    on_tablet_connected(&dev.serial, sess.video_port, sess.input_port, auto_launch, token.as_deref()).await;
                    if dev.transport == Transport::Usb {
                        auto_provision_wifi(&dev.serial).await;
                    }
                }
                let _ = sess.tablet_tx.send(true);
                extras.insert(hw_id.clone(), sess);
            }
        }

        let extra_relaunch = async {
            let notifies: Vec<_> = extras.values().map(|s| s.relaunch.clone()).collect();
            if notifies.is_empty() {
                std::future::pending::<()>().await;
            }
            let futs = notifies.iter().map(|n| Box::pin(n.notified()));
            futures_util::future::select_all(futs).await;
        };

        tokio::select! {
            Some(msg) = control_rx.recv() => {
                match msg {
                    DaemonControlMsg::PauseDevice { serial, resp } => {
                        info!("Received surgical PAUSE request for device {}", serial);
                        paused_serials.insert(serial.clone());
                        if slot_0_active_serial == serial {
                            let _ = tablet_tx.send(false);
                            let fifo = crate::capture::fifo_path_for(0);
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("evdi_helper .*--capture-fifo {}( |$)", fifo)])
                                .status().await;
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("ffmpeg.*{}", fifo)])
                                .status().await;
                            crate::capture::CaptureManager::disable_evdi_display(vdisplay::evdi_cards().first().copied()).await;
                            let _ = resp.send(Ok(()));
                        } else if let Some(sess) = extras.values_mut().find(|s| s.active_serial == serial) {
                            let _ = sess.tablet_tx.send(false);
                            let fifo = crate::capture::fifo_path_for(sess.instance);
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("evdi_helper .*--capture-fifo {}( |$)", fifo)])
                                .status().await;
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("ffmpeg.*{}", fifo)])
                                .status().await;
                            let cards = vdisplay::evdi_cards();
                            let card = cards.get(sess.instance as usize).copied();
                            crate::capture::CaptureManager::disable_evdi_display(card).await;
                            let _ = resp.send(Ok(()));
                        } else {
                            let fifo = crate::capture::fifo_path_for(0);
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("evdi_helper .*--capture-fifo {}( |$)", fifo)])
                                .status().await;
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("ffmpeg.*{}", fifo)])
                                .status().await;
                            let _ = resp.send(Ok(()));
                        }
                    }
                    DaemonControlMsg::ResumeDevice { serial, resp } => {
                        info!("Received surgical RESUME request for device {}", serial);
                        paused_serials.remove(&serial);
                        if slot_0_active_serial == serial {
                            let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                            launch_app(&slot_0_active_serial, token.as_deref(), video_port, input_port).await;
                            let _ = tablet_tx.send(true);
                            let _ = resp.send(Ok(()));
                        } else if let Some(sess) = extras.values_mut().find(|s| s.active_serial == serial) {
                            let _ = setup_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                            launch_app(&sess.active_serial, token.as_deref(), sess.video_port, sess.input_port).await;
                            let _ = sess.tablet_tx.send(true);
                            let _ = resp.send(Ok(()));
                        } else {
                            let _ = resp.send(Err(format!("Device {} not found in active sessions", serial)));
                        }
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {
                polls_since_check += 1;
                if polls_since_check < APP_CHECK_EVERY {
                    continue;
                }
                polls_since_check = 0;

                // A. Background Wi-Fi Reconnection Watchdog: ensure all configured devices connect
                let file_cfg = config::FileConfig::load();
                let mut targets = Vec::new();
                if !file_cfg.wifi_address.is_empty() {
                    targets.push(file_cfg.wifi_address.clone());
                }
                if !wifi_address.is_empty() && !targets.contains(&wifi_address) {
                    targets.push(wifi_address.clone());
                }
                for (_k, p) in &file_cfg.tablets {
                    if let Some(ref w) = p.wifi_address {
                        if !targets.contains(w) {
                            targets.push(w.clone());
                        }
                    }
                }
                for target in targets {
                    let already_connected = all_devices.iter().any(|d| d.serial == target);
                    if !already_connected {
                        tokio::spawn(async move {
                            let _ = tokio::process::Command::new("adb")
                                .args(["connect", &target])
                                .output()
                                .await;
                        });
                    }
                }

                // B. App Running Watchdog for Slot 0
                if auto_launch && !slot_0_active_serial.is_empty() && !is_fake_serial(&slot_0_active_serial) && !paused_serials.contains(&slot_0_active_serial) {
                    match app_running(&slot_0_active_serial).await {
                        Some(true) => {
                            slot_0_relaunch_wait = std::time::Duration::from_secs(5);
                        }
                        Some(false) if slot_0_last_relaunch.elapsed() >= slot_0_relaunch_wait => {
                            slot_0_last_relaunch = std::time::Instant::now();
                            info!("UScreen app not running on Slot 1 ({}) — re-verifying port forwarding and launching it", slot_0_active_serial);
                            let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                            launch_app(&slot_0_active_serial, token.as_deref(), video_port, input_port).await;
                            slot_0_relaunch_wait = (slot_0_relaunch_wait * 2).min(RELAUNCH_WAIT_MAX);
                        }
                        _ => {}
                    }
                }

                // C. App Running Watchdog for ALL Extra Sessions
                if auto_launch {
                    for sess in extras.values_mut() {
                        if is_fake_serial(&sess.active_serial) || paused_serials.contains(&sess.active_serial) { continue; }
                        match app_running(&sess.active_serial).await {
                            Some(true) => {
                                sess.relaunch_wait = std::time::Duration::from_secs(5);
                            }
                            Some(false) if sess.last_relaunch.elapsed() >= sess.relaunch_wait => {
                                sess.last_relaunch = std::time::Instant::now();
                                info!("UScreen app not running on Slot {} ({}) — re-verifying port forwarding and launching it", sess.instance + 1, sess.active_serial);
                                let _ = setup_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                                launch_app(&sess.active_serial, token.as_deref(), sess.video_port, sess.input_port).await;
                                sess.relaunch_wait = (sess.relaunch_wait * 2).min(RELAUNCH_WAIT_MAX);
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ = relaunch.notified() => {
                if !slot_0_active_serial.is_empty() && !paused_serials.contains(&slot_0_active_serial) {
                    if slot_0_last_relaunch.elapsed() >= slot_0_relaunch_wait {
                        slot_0_last_relaunch = std::time::Instant::now();
                        slot_0_relaunches += 1;
                        if slot_0_relaunches == 3 {
                            warn!("A client keeps connecting without a valid token. Update the UScreen app on the tablet (1.1.0 or newer).");
                        }
                        info!("Delivering session token to Slot 1 app (attempt {}, next in {:?})", slot_0_relaunches, slot_0_relaunch_wait.min(RELAUNCH_WAIT_MAX));
                        let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                        launch_app(&slot_0_active_serial, token.as_deref(), video_port, input_port).await;
                        slot_0_relaunch_wait = (slot_0_relaunch_wait * 2).min(RELAUNCH_WAIT_MAX);
                    }
                }
            }
            _ = extra_relaunch => {
                for sess in extras.values_mut() {
                    if is_fake_serial(&sess.active_serial) || paused_serials.contains(&sess.active_serial) { continue; }
                    if sess.last_relaunch.elapsed() >= sess.relaunch_wait {
                        sess.last_relaunch = std::time::Instant::now();
                        sess.relaunches += 1;
                        info!("Delivering session token to Slot {} app ({})", sess.instance + 1, sess.active_serial);
                        let _ = setup_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                        launch_app(&sess.active_serial, token.as_deref(), sess.video_port, sess.input_port).await;
                        sess.relaunch_wait = (sess.relaunch_wait * 2).min(RELAUNCH_WAIT_MAX);
                    }
                }
            }
        }
    }
}

/// Switch the tablet's adb to TCP and remember where it lives, so the daemon
/// can pick it up over Wi-Fi on its own from then on.
///
/// This is deliberately the adb route rather than a port of our own. The
/// video and input ports stay on loopback, reachable only through the tunnel
/// adb builds, so nothing new is exposed to the network and the tablet still
/// has to be a device this computer is authorised to talk to.
async fn setup_wifi(off: bool) -> Result<()> {
    let mut cfg = config::FileConfig::load();

    if off {
        if !cfg.wifi_address.is_empty() {
            let _ = tokio::process::Command::new("adb")
                .args(["disconnect", &cfg.wifi_address])
                .output()
                .await;
        }
        cfg.wifi_address = String::new();
        cfg.save()?;
        println!("Wi-Fi off. Plug the cable in to use the tablet again.");
        return Ok(());
    }

    let Some(serial) = adb_devices().await.into_iter().find(|s| transport_of(s) == Transport::Usb)
    else {
        anyhow::bail!(
            "No tablet on USB. Plug the cable in for this one step — the tablet has to be told \
             to listen on the network, and only the cable can tell it."
        );
    };

    println!("Switching {} to Wi-Fi…", serial);
    let out = tokio::process::Command::new("adb")
        .args(["-s", &serial, "tcpip", "5555"])
        .output()
        .await?;
    if !out.status.success() {
        anyhow::bail!("adb tcpip failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    // adbd restarts, taking the USB connection with it for a moment.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let Some(ip) = tablet_ip(&serial).await else {
        anyhow::bail!(
            "The tablet is listening, but its address could not be read. Find it under \
             Settings → About tablet → Status, then put `wifi_address = \"<ip>:5555\"` in \
             ~/.config/uscreen/config.toml."
        );
    };
    let address = format!("{}:5555", ip);

    let out = tokio::process::Command::new("adb").args(["connect", &address]).output().await?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !said.contains("connected") {
        anyhow::bail!("adb connect {} did not take: {}", address, said);
    }

    cfg.wifi_address = address.clone();
    cfg.save()?;
    println!("Connected to {}. The cable can come out.", address);
    println!(
        "The daemon reconnects to this address by itself whenever the cable is not in, \
         so this is a one-off — until the tablet reboots, which puts its adb back on USB \
         and means running this once more.\n\
         Wi-Fi is a fallback: the median latency matches the cable but single frames \
         arrive much later. `uscreen wifi --off` forgets the address."
    );
    Ok(())
}

/// The tablet's own address on the wireless network.
async fn tablet_ip(serial: &str) -> Option<String> {
    // `ip route` is present on every Android that adb can reach, and the
    // route to the default gateway carries the source address we want.
    // Asked for wlan0 first, since a tablet on USB may also have a tethering
    // interface whose address is useless here.
    for args in [
        vec!["-s", serial, "shell", "ip", "-f", "inet", "addr", "show", "wlan0"],
        vec!["-s", serial, "shell", "ip", "route", "get", "1.1.1.1"],
        vec!["-s", serial, "shell", "ip", "-f", "inet", "addr"],
    ] {
        let Ok(out) = tokio::process::Command::new("adb").args(&args).output().await else {
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        // "inet 192.168.1.42/24 …" or "… src 192.168.1.42 …"
        let mut words = text.split_whitespace().peekable();
        while let Some(w) = words.next() {
            if w != "inet" && w != "src" {
                continue;
            }
            let Some(value) = words.peek() else { continue };
            let ip = value.split('/').next().unwrap_or(value);
            if ip.starts_with("127.") || !ip.contains('.') {
                continue;
            }
            if ip.split('.').count() == 4 && ip.split('.').all(|o| o.parse::<u8>().is_ok()) {
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// Automatically enable `adb tcpip 5555` in the background and discover the device's
/// local Wi-Fi IP address when connected over USB, saving it for seamless handoff.
async fn auto_provision_wifi(serial: &str) {
    if transport_of(serial) != Transport::Usb || is_fake_serial(serial) {
        return;
    }
    let s = serial.to_string();
    tokio::spawn(async move {
        // If Wi-Fi is already known for this device, avoid running `adb tcpip 5555`
        // which resets Android's adbd and causes a disruptive USB bus re-enumeration.
        let cfg = config::FileConfig::load();
        if let Some(profile) = cfg.tablets.get(&s) {
            if let Some(ref addr) = profile.wifi_address {
                let _ = tokio::process::Command::new("adb")
                    .args(["connect", addr])
                    .output()
                    .await;
                return;
            }
        }

        let out = tokio::process::Command::new("adb")
            .args(["-s", &s, "tcpip", "5555"])
            .output()
            .await;
        if out.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            info!("Auto-enabled adb tcpip 5555 for USB device {}", s);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Some(ip) = tablet_ip(&s).await {
                let addr = format!("{}:5555", ip);
                info!("Discovered Wi-Fi address for device {}: {}", s, addr);
                let mut cfg = config::FileConfig::load();
                let profile = cfg.tablets.entry(s.clone()).or_default();
                if profile.wifi_address.as_ref() != Some(&addr) {
                    profile.wifi_address = Some(addr.clone());
                    let _ = cfg.save();
                }
                // Pre-connect in background so Wi-Fi transport is hot and ready for handoff
                let _ = tokio::process::Command::new("adb")
                    .args(["connect", &addr])
                    .output()
                    .await;
            }
        }
    });
}

/// Synchronize the application window lock state with the GNOME Shell extension via JSON state.
pub fn sync_app_lock(connector: Option<&str>, serial: Option<&str>) {
    let cfg = config::FileConfig::load();
    let apps = serial
        .and_then(|s| cfg.tablets.get(s))
        .and_then(|p| p.locked_apps.clone())
        .unwrap_or(cfg.locked_apps);

    let home = std::env::var("HOME").unwrap_or_default();
    let dir = std::path::PathBuf::from(home).join(".local/share/uscreen");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("app_lock.json");

    let enabled = !apps.is_empty() && connector.is_some();
    let json = serde_json::json!({
        "enabled": enabled,
        "connector": connector.unwrap_or(""),
        "locked_apps": apps,
    });
    if let Ok(content) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(path, content);
    }
}

/// Set up the display this tablet had last time, before anything is built.
///
/// Without it the helper comes up with the previous tablet's mode and EDID,
/// the app reports the real panel a second later, and the whole pipeline is
/// torn down and rebuilt — twice, since the physical size arrives with it.
/// On a slow machine that was half a minute of black screen at every start.
async fn apply_tablet_profile(serial: &str, settings_tx: &watch::Sender<capture::EncoderSettings>) {
    let mut cfg = config::FileConfig::load();
    let Some(mut p) = cfg.tablets.get(serial).cloned() else {
        // First time this tablet is seen: note it, so the file shows what is
        // known even before it has reported anything.
        if !is_fake_serial(serial) {
            let label = tokio::process::Command::new("adb")
                .args(["-s", serial, "shell", "getprop", "ro.product.model"])
                .output()
                .await
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            cfg.tablets.entry(serial.to_string()).or_default().label = label;
            let _ = cfg.save();
        }
        return;
    };
    if p.label.trim().is_empty() && !is_fake_serial(serial) {
        if let Ok(o) = tokio::process::Command::new("adb")
            .args(["-s", serial, "shell", "getprop", "ro.product.model"])
            .output()
            .await
        {
            let label = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !label.is_empty() {
                p.label = label.clone();
                cfg.tablets.entry(serial.to_string()).or_default().label = label;
                let _ = cfg.save();
            }
        }
    }
    let mut s = settings_tx.borrow().clone();
    let before = s.clone();
    if p.has_display() {
        s.width = p.width;
        s.height = p.height;
        if p.width_mm > 0 && p.height_mm > 0 {
            s.width_mm = p.width_mm;
            s.height_mm = p.height_mm;
        }
    }
    s.fps = p.fps.unwrap_or(cfg.fps);
    s.bitrate = p.bitrate.unwrap_or(cfg.bitrate);
    s.quality = p.quality.unwrap_or(cfg.quality);
    s.stream_scale = p.stream_scale.unwrap_or(cfg.stream_scale).max(1);
    s.encoder = p.encoder.as_ref().filter(|e| !e.trim().is_empty()).cloned().unwrap_or(cfg.encoder.clone());
    s.ten_bit = p.ten_bit.unwrap_or(cfg.ten_bit);
    s.keep_alive = p.keep_alive.unwrap_or(cfg.keep_alive);

    if before == s {
        return;
    }
    info!(
        "Device profile applied for {} ({}): {}x{}@{}fps, encoder: {}, bitrate: {}k, quality: {}, scale: {}, 10-bit: {}",
        p.display_name(serial),
        serial,
        s.width, s.height, s.fps,
        s.encoder,
        s.bitrate,
        s.quality,
        s.stream_scale,
        s.ten_bit,
    );
    let _ = settings_tx.send(s);
}

/// Whether the app's process exists on the tablet. `None` when adb could not
/// answer (cable pulled mid-check, adb restarting), so the caller does nothing
/// rather than launching on a guess.
async fn app_running(serial: &str) -> Option<bool> {
    let out = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell", "pidof", "com.uscreen"])
        .output()
        .await
        .ok()?;
    // pidof exits 1 with no output when nothing matches; adb itself failing
    // shows up as a non-empty stderr.
    if !out.stderr.is_empty() {
        return None;
    }
    Some(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Measured on a quiet network: the median roughly doubles, but the 95th
/// percentile goes from about 28ms to over 150ms and individual frames have
/// been seen at three quarters of a second. Worth saying out loud, because
/// "it works" and "it is pleasant to draw on" are not the same claim.
fn announce_transport(serial: &str) {
    if transport_of(serial) == Transport::Network {
        warn!("Running over Wi-Fi. Expect occasional stutter — the cable is much steadier.");
    }
}

async fn on_tablet_connected(
    serial: &str,
    video_port: u16,
    input_port: u16,
    auto_launch: bool,
    token: Option<&str>,
) {
    match setup_adb_forwarding(serial, video_port, input_port).await {
        Ok(_) => {
            info!("ADB port forwarding set up (video={}, input={})", video_port, input_port);
            if auto_launch {
                launch_app(serial, token, video_port, input_port).await;
            }
        }
        Err(e) => warn!("ADB forwarding failed: {}", e),
    }
}

/// Start (or re-front) the app, handing it the session token as an intent
/// extra along with unique per-device video and input ports.
async fn launch_app(serial: &str, token: Option<&str>, video_port: u16, input_port: u16) {
    use tokio::io::AsyncWriteExt;
    let mut cmd = format!(
        "am start -n com.uscreen/.MainActivity --ei video_port {} --ei input_port {}",
        video_port, input_port
    );
    if let Some(t) = token {
        // Hex only, so no quoting is needed and nothing can break out.
        cmd.push_str(" --es token ");
        cmd.push_str(t);
    }
    cmd.push_str(" >/dev/null 2>&1; exit\n");

    let child = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            warn!("Could not run adb: {}", e);
            return;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(cmd.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    match tokio::time::timeout(std::time::Duration::from_secs(15), child.wait()).await {
        Ok(Ok(st)) if st.success() => info!("UScreen app launched on tablet"),
        _ => warn!("Could not launch the app (is it installed?)"),
    }
}

/// Serial of the first fully-online device, or `None`.
///
/// Deliberately not `adb get-state`: that command fails outright with
/// "more than one device/emulator" as soon as a second device (a phone, an
/// emulator) is attached, which used to turn plug-and-play off with no
/// indication of why.
/// How the tablet is reached. Nothing in the pipeline is tied to either — it
/// speaks to whatever adb is connected to — but the difference is worth a
/// dozen milliseconds, so it is worth naming.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Transport {
    Usb,
    Network,
}

impl Transport {
    fn label(self) -> &'static str {
        match self {
            Transport::Usb => "USB",
            Transport::Network => "Wi-Fi",
        }
    }
}

/// A network device's serial is its `host:port`, or — for Android 11's
/// wireless debugging, which adb finds by mDNS — a name like
/// `adb-R52WA0A0KED-aBcDeF._adb-tls-connect._tcp`, which has no colon. A USB
/// serial is neither. That is the whole distinction adb gives us without a
/// second call.
pub fn is_network_serial(serial: &str) -> bool {
    serial.contains(':') || serial.contains("._adb")
}

fn transport_of(serial: &str) -> Transport {
    if is_network_serial(serial) {
        Transport::Network
    } else {
        Transport::Usb
    }
}

/// Pick the tablet to drive, preferring USB.
///
/// Both can be present at once — `adb tcpip` leaves the cable working — and
/// the order adb happens to list them in is not something to hang a latency
/// difference on. USB wins whenever it is there.
fn is_fake_serial(serial: &str) -> bool {
    std::env::var("USCREEN_FAKE_TABLET")
        .map(|f| f.split(',').any(|x| x.trim() == serial))
        .unwrap_or(false)
}

/// Which of the attached devices to drive.
///
/// Stay with the one already in use for as long as it is still attached.
/// adb lists two devices in no fixed order, and taking `first()` on every
/// poll meant that any reshuffle — a phone's USB re-enumerating when its
/// screen sleeps is enough — looked like a different tablet being plugged in:
/// the port forwards moved, the stream on the real tablet froze on its last
/// frame, and the app went black when it reconnected. That is the shape of
/// the black screen in #10, reported with two Samsung devices attached.
///
/// For a fresh pick, prefer USB over the network, and among USB devices the
/// one that actually has the app installed: a phone charging next to the
/// tablet normally does not, and it is almost never the one meant.
#[allow(dead_code)]
async fn pick_device(devices: &[String], current: Option<&str>) -> Option<String> {
    if let Some(cur) = current {
        if devices.iter().any(|d| d == cur) {
            return Some(cur.to_string());
        }
    }
    let usb: Vec<&String> = devices.iter().filter(|d| transport_of(d) == Transport::Usb).collect();
    if usb.len() > 1 {
        for d in &usb {
            if is_fake_serial(d) {
                continue;
            }
            let has_app = tokio::process::Command::new("adb")
                .args(["-s", d, "shell", "pm", "path", "com.uscreen"])
                .output()
                .await
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);
            if has_app {
                info!("{} Android devices attached; using {} — it has the UScreen app", devices.len(), d);
                return Some((*d).clone());
            }
        }
        warn!(
            "{} Android devices attached and none has the app installed; using {}. \
             Unplug the others, or raise max_tablets to give each its own screen.",
            devices.len(),
            usb[0]
        );
    }
    devices.first().cloned()
}

/// Every device in state "device", USB entries first.
async fn adb_devices() -> Vec<String> {
    let Ok(out) = tokio::process::Command::new("adb").arg("devices").output().await else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut ready: Vec<String> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            (state == "device").then(|| serial.to_string())
        })
        .collect();
    ready.sort_by_key(|s| transport_of(s) != Transport::Usb);
    ready
}

#[allow(dead_code)]
async fn adb_device_serial() -> Option<String> {
    let out = tokio::process::Command::new("adb")
        .arg("devices")
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let ready: Vec<&str> = text
        .lines()
        .skip(1) // "List of devices attached"
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            (state == "device").then_some(serial)
        })
        .collect();

    ready
        .iter()
        .find(|s| transport_of(s) == Transport::Usb)
        .or_else(|| ready.first())
        .map(|s| s.to_string())
}

async fn setup_adb_forwarding(serial: &str, video_port: u16, input_port: u16) -> Result<()> {
    info!("[ADB-FORWARD] Setting up reverse port forwarding for {} (video={}, input={})", serial, video_port, input_port);
    // Forward unique ports directly with retry
    for port in [video_port, input_port] {
        let arg = format!("tcp:{}", port);
        let mut last_err = String::new();
        let mut success = false;
        for attempt in 1..=3 {
            let r = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                tokio::process::Command::new("adb")
                    .args(["-s", serial, "reverse", &arg, &arg])
                    .output(),
            )
            .await;
            match r {
                Ok(Ok(output)) if output.status.success() => {
                    success = true;
                    break;
                }
                Ok(Ok(output)) => {
                    last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
                Ok(Err(e)) => {
                    last_err = e.to_string();
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
                Err(_) => {
                    last_err = format!("adb reverse {} timed out", arg);
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
        if !success {
            anyhow::bail!("adb reverse {} failed after 3 attempts: {}", arg, last_err);
        }
    }
    // Only map fallback 8890/8891 for slot 0 to avoid multi-device port cross-talk
    if video_port == 8890 {
        let _ = tokio::process::Command::new("adb")
            .args(["-s", serial, "reverse", "tcp:8890", "tcp:8890"])
            .output()
            .await;
        let _ = tokio::process::Command::new("adb")
            .args(["-s", serial, "reverse", "tcp:8891", "tcp:8891"])
            .output()
            .await;
    }
    info!("[ADB-FORWARD] Successfully established reverse port forwarding for {}", serial);
    Ok(())
}

async fn teardown_adb_forwarding(serial: &str, video_port: u16, input_port: u16) {
    info!("[ADB-FORWARD] Tearing down reverse port forwarding for {} (video={}, input={})", serial, video_port, input_port);
    let mut ports_to_remove = vec![video_port, input_port];
    if video_port == 8890 {
        ports_to_remove.push(8890);
        ports_to_remove.push(8891);
    }
    ports_to_remove.sort_unstable();
    ports_to_remove.dedup();
    for port in ports_to_remove {
        let arg = format!("tcp:{}", port);
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            tokio::process::Command::new("adb")
                .args(["-s", serial, "reverse", "--remove", &arg])
                .output(),
        )
        .await;
    }
}

async fn stop_daemon() -> Result<()> {
    let pid_path = get_pid_path();

    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path)?;
        let pid: u32 = pid_str.trim().parse()?;
        info!("Stopping uscreen daemon (PID: {})", pid);

        let result = tokio::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output()
            .await?;

        if result.status.success() {
            let _ = std::fs::remove_file(&pid_path);
            info!("uscreen daemon stopped");
        } else {
            // Fallback: try pkill but exclude our own PID
            for p in other_daemons() {
                unsafe { libc::kill(p as i32, libc::SIGTERM); }
            }
            let _ = std::fs::remove_file(&pid_path);
            info!("uscreen daemon stopped (fallback)");
        }
    } else {
        // No PID file, try pkill but exclude self
        for p in other_daemons() {
            unsafe { libc::kill(p as i32, libc::SIGTERM); }
        }
        info!("uscreen daemon stopped (no PID file)");
    }

    Ok(())
}

async fn show_status() -> Result<()> {
    let pid_path = get_pid_path();
    let my_pid = std::process::id();

    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path)?;
        let pid: u32 = pid_str.trim().parse().unwrap_or(0);

        if pid > 0 && pid != my_pid {
            // Check if the process is actually running
            let proc_path = format!("/proc/{}", pid);
            if std::path::Path::new(&proc_path).exists() {
                println!("uscreen is running (PID: {}, version: {})", pid, env!("GIT_VERSION_STRING"));
            } else {
                println!("uscreen is not running (stale PID file)");
                let _ = std::fs::remove_file(&pid_path);
            }
        } else {
            println!("uscreen is not running (version: {})", env!("GIT_VERSION_STRING"));
        }
    } else {
        // Fallback: pgrep excluding self
        let pids = other_daemons()
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if !pids.is_empty() {
            println!("uscreen is running (PID: {}, version: {})", pids, env!("GIT_VERSION_STRING"));
        } else {
            println!("uscreen is not running (version: {})", env!("GIT_VERSION_STRING"));
        }
    }
    Ok(())
}

async fn list_displays() -> Result<()> {
    println!("=== Available displays ===");
    if let Ok(out) = tokio::process::Command::new("kscreen-doctor")
        .args(["-o"])
        .output()
        .await
    {
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }

    if let Ok(out) = tokio::process::Command::new("wpctl")
        .args(["status"])
        .output()
        .await
    {
        println!("--- PipeWire ---");
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_network_serials_from_usb_ones() {
        assert!(is_network_serial("192.168.1.20:5555"));
        assert!(is_network_serial("adb-R52WA0A0KED-aBcDeF._adb-tls-connect._tcp"));
        assert!(is_network_serial("adb-R52WA0A0KED-aBcDeF._adb-tls-pairing._tcp"));
        assert!(!is_network_serial("R52WA0A0KED"));
        assert!(!is_network_serial("emulator-5554"));
        assert_eq!(transport_of("R52WA0A0KED"), Transport::Usb);
        assert_eq!(transport_of("adb-X-y._adb-tls-connect._tcp"), Transport::Network);
    }

    #[test]
    fn physical_device_deduplication_prefers_usb() {
        let dev1 = DiscoveredDevice {
            serial: "192.168.1.74:5555".to_string(),
            state: "device".to_string(),
            transport: Transport::Network,
            hw_id: "RZCX809Y0FT".to_string(),
        };
        let dev2 = DiscoveredDevice {
            serial: "RZCX809Y0FT".to_string(),
            state: "device".to_string(),
            transport: Transport::Usb,
            hw_id: "RZCX809Y0FT".to_string(),
        };
        let dev3 = DiscoveredDevice {
            serial: "R52X708AEHK".to_string(),
            state: "device".to_string(),
            transport: Transport::Usb,
            hw_id: "R52X708AEHK".to_string(),
        };

        let mut map: std::collections::HashMap<String, &DiscoveredDevice> = std::collections::HashMap::new();
        for dev in &[&dev1, &dev2, &dev3] {
            map.entry(dev.hw_id.clone())
                .and_modify(|existing| {
                    if existing.transport != Transport::Usb && dev.transport == Transport::Usb {
                        *existing = dev;
                    }
                })
                .or_insert(dev);
        }

        // Must collapse to 2 distinct physical devices
        assert_eq!(map.len(), 2);
        // RZCX809Y0FT must choose the USB endpoint over Network
        assert_eq!(map.get("RZCX809Y0FT").unwrap().serial, "RZCX809Y0FT");
        assert_eq!(map.get("RZCX809Y0FT").unwrap().transport, Transport::Usb);
        // R52X708AEHK must be intact
        assert_eq!(map.get("R52X708AEHK").unwrap().serial, "R52X708AEHK");
    }
}
