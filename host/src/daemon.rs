//! The daemon proper: `uscreen start` (without `--daemonize`) runs [`run_daemon`] until SIGTERM,
//! SIGINT, or a stop request over the control socket.
//!
//! It wires the pieces together: capture pipelines, the stream and input servers, the ADB device
//! monitor, the tray icon and the Unix control socket the GUI talks to.

use anyhow::Result;
use tokio::signal;
use tokio::sync::{broadcast, mpsc, watch};
use tracing::{error, info, warn};

use crate::cli::Cli;
use crate::config_handler::{
    find_helper, get_pid_path, other_daemons, remove_pid_file_if_ours, run_control_server,
    sync_app_lock,
};
use crate::display::{edid, vdisplay};
use crate::net::stream;
use crate::session::{adb_monitor, DaemonControlMsg, ExtraSessionTemplate};
use crate::{capture, config, encoders, input, options, osk, runtime, tray, update};

/// Execute the primary uscreen daemon event loop.
///
/// Coordinates hardware video capture, hardware uinput forwarding, Unix control socket,
/// and ADB device orchestration until terminated by SIGTERM, SIGINT, or GUI quit command.
pub(crate) async fn run_daemon(cli: Cli) -> Result<()> {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }

    let pid_path = get_pid_path();
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

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

    let others = other_daemons();
    if !others.is_empty() {
        anyhow::bail!(
            "uscreen daemon already running (PID {:?}, untracked). Run `uscreen stop` first.",
            others
        );
    }

    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())?;

    let file_cfg = config::FileConfig::load();

    {
        let raw: Option<config::FileConfig> = std::fs::read_to_string(config::config_path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok());
        if raw.is_some_and(|r| r != file_cfg) {
            match file_cfg.save() {
                Ok(_) => info!(
                    "Rewrote out-of-range settings in {:?}",
                    config::config_path()
                ),
                Err(e) => warn!("Could not rewrite the config file: {}", e),
            }
        }
    }

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
    if cli.pen_only {
        options::set_cli_mode(config::InputMode::Stylus);
    }
    let pen_only = cli.pen_only || file_cfg.input_mode == Some(config::InputMode::Stylus);
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
        width_mm: edid::DEFAULT_WIDTH_MM,
        height_mm: edid::DEFAULT_HEIGHT_MM,
        stream_scale,
        position: config::Position::parse_or_default(&file_cfg.position),
        ten_bit: file_cfg.ten_bit,
        instance: 0,
        card: vdisplay::evdi_cards().first().copied(),
        max_performance_mode,
        keep_alive: file_cfg.keep_alive,
    };

    let token: Option<String> = if file_cfg.require_token {
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

    let stream_config = stream::StreamConfig {
        video_port,
        token: token.clone(),
    };

    let input_config = input::InputConfig {
        port: input_port,
        instance: 0,
        token: token.clone(),
        codec: capture::Codec::from_encoder(&encoder).name().to_string(),
        virtual_width: width,
        virtual_height: height,
        pointer_handoff: file_cfg.pointer_handoff,
    };

    let (mode_tx, _) = watch::channel(pen_only);

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
        restart_nonce: 0,
    });

    let mut capture_mgr = capture::CaptureManager::new(cap_config);
    let card_rx = capture_mgr.card_rx();
    let codec_config = capture_mgr.codec_config_arc();
    let latency = capture_mgr.latency_tracker();
    let stream_srv =
        stream::StreamServer::new(stream_config, codec_config, capture_mgr.idr_request_flag());
    let ping_notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let input_srv = input::InputServer::new(
        input_config,
        Some(settings_tx.clone()),
        latency,
        relaunch.clone(),
        card_rx.clone(),
        ping_notify.clone(),
    );

    let (video_tx, _) = broadcast::channel(8);

    osk::disable().await;

    info!("=== uscreen daemon starting ===");
    info!("  Resolution: {}x{} @ {}fps", width, height, fps);
    info!("  Encoder: {}", encoder);
    info!("  Bitrate: {} kbps", bitrate);
    info!("  Stream port: {}", video_port);
    info!("  Input port: {}", input_port);

    let (tablet_tx, mut tablet_rx) = watch::channel(false);
    let tray_tablet_rx = tablet_tx.subscribe();

    let (gate_tx, gate_rx) = watch::channel(false);
    // Only Display mode runs the pipeline: Touchpad and Stylus mode keep it down (no EVDI capture,
    // no ffmpeg, nothing on the USB link but input).
    options::spawn_config_watcher();
    let mut touchpad_rx_gate = options::options_rx(0);
    tokio::spawn(async move {
        let mut last = false;
        loop {
            let streams = touchpad_rx_gate.borrow().input_mode.streams_video();
            let active = *tablet_rx.borrow() && streams;
            if active != last {
                last = active;
                let _ = gate_tx.send(active);
            }
            tokio::select! {
                r = tablet_rx.changed() => if r.is_err() { break },
                r = touchpad_rx_gate.changed() => if r.is_err() { break },
            }
        }
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    if pen_only {
        info!("Starting in pen-only mode: the device drives this machine's own");
        info!("  screen with the pen. No virtual display, no encoding.");
    }

    let video_tx_cap = video_tx.clone();
    let settings_rx_cap = settings_rx.clone();
    let cap_handle = tokio::spawn(async move {
        if let Err(e) = capture_mgr
            .stream_frames(video_tx_cap, settings_rx_cap, gate_rx, shutdown_rx)
            .await
        {
            error!("Capture manager failed: {}", e);
        }
    });

    let stream_srv = stream_srv.with_shutdown(shutdown_tx.subscribe());
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

    let cli_overrides = (
        cli.encoder.is_some(),
        cli.fps.is_some(),
        cli.bitrate.is_some(),
        cli.width.is_some(),
        cli.height.is_some(),
        cli.quality.is_some(),
        cli.stream_scale.is_some(),
    );

    let (serial_tx, serial_rx) = watch::channel(None::<String>);

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

    let serial_rx_save = serial_rx.clone();
    let mut settings_rx_save = settings_rx.clone();
    let save_handle = tokio::spawn(async move {
        while settings_rx_save.changed().await.is_ok() {
            let s = settings_rx_save.borrow().clone();
            let mut cfg = config::FileConfig::load();
            let (
                has_enc_override,
                has_fps_override,
                has_bitrate_override,
                has_quality_override,
                has_scale_override,
                has_ten_bit_override,
            ) = if let Some(serial) = serial_rx_save.borrow().as_ref() {
                if let Some(p) = cfg.tablets.get(serial) {
                    (
                        p.encoder.is_some(),
                        p.fps.is_some(),
                        p.bitrate.is_some(),
                        p.quality.is_some(),
                        p.stream_scale.is_some(),
                        p.ten_bit.is_some(),
                    )
                } else {
                    (false, false, false, false, false, false)
                }
            } else {
                (false, false, false, false, false, false)
            };

            if !cli_overrides.0 && !has_enc_override {
                cfg.encoder = s.encoder;
            }
            if !cli_overrides.1 && !has_fps_override {
                cfg.fps = s.fps;
            }
            if !cli_overrides.2 && !has_bitrate_override {
                cfg.bitrate = s.bitrate;
            }
            if !cli_overrides.3 {
                cfg.width = s.width;
            }
            if !cli_overrides.4 {
                cfg.height = s.height;
            }
            if !cli_overrides.5 && !has_quality_override {
                cfg.quality = s.quality;
            }
            if !cli_overrides.6 && !has_scale_override {
                cfg.stream_scale = s.stream_scale;
            }
            if !has_ten_bit_override {
                cfg.ten_bit = s.ten_bit;
            }
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

    // Keep the tray's switch in step with slot 0's real mode (a GUI or tablet change).
    {
        let mut slot0_pen = options::pen_only_rx(0);
        let tray_tx = mode_tx.clone();
        tokio::spawn(async move {
            while slot0_pen.changed().await.is_ok() {
                let stylus = *slot0_pen.borrow();
                tray_tx.send_if_modified(|cur| {
                    let changed = *cur != stylus;
                    *cur = stylus;
                    changed
                });
            }
        });
    }
    // The tray's "Graphics tablet" switch asks for a new *global* default here. Only that: a mode
    // change that merely mirrors slot 0 (a tablet's own setting, a per-device override from the GUI)
    // must never be written back as the global default, which is what used to happen.
    let (mode_request_tx, mut mode_request_rx) = tokio::sync::mpsc::unbounded_channel::<bool>();
    let mode_save_handle = tokio::spawn(async move {
        while let Some(pen_only) = mode_request_rx.recv().await {
            let mut cfg = config::FileConfig::load();
            if !cfg.set_global_stylus(pen_only) {
                continue;
            }
            if let Err(e) = cfg.save() {
                warn!("Failed to persist mode: {}", e);
            }
        }
    });

    let (update_tx, update_rx) = watch::channel::<update::Available>(None);
    let update_handle = if file_cfg.check_updates {
        Some(tokio::spawn(async move { update::run(update_tx).await }))
    } else {
        None
    };

    let tray_mode_tx = mode_tx.clone();
    let tray_shutdown_tx = shutdown_tx.clone();
    let tray_handle = tokio::spawn(async move {
        tray::run(
            tray_mode_tx,
            mode_request_tx,
            tray_tablet_rx,
            tray_shutdown_tx,
            update_rx,
        )
        .await;
    });

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
            keep_alive: file_cfg.keep_alive,
        },
        video_port,
        input_port,
        token: token.clone(),
        shutdown_rx: shutdown_tx.subscribe(),
        ping_notify: ping_notify.clone(),
    };
    let wifi_address = file_cfg.wifi_address.clone();
    let settings_tx_adb = settings_tx.clone();
    let (control_tx, control_rx) = mpsc::channel::<DaemonControlMsg>(16);
    let adb_ping = ping_notify.clone();
    let adb_handle = tokio::spawn(async move {
        adb_monitor(
            video_port,
            input_port,
            auto_launch,
            tablet_tx,
            adb_token,
            relaunch,
            extra,
            wifi_address,
            settings_tx_adb,
            serial_tx,
            control_rx,
            adb_ping,
        )
        .await;
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
    println!("  On your device, open the UScreen app");
    println!("  ADB ports will be auto-forwarded if possible.");
    println!("  Otherwise, run:");
    println!("    adb reverse tcp:8890 tcp:8890");
    println!("    adb reverse tcp:8891 tcp:8891");
    println!("================================================");
    println!();

    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut quit_rx = shutdown_tx.subscribe();
    tokio::select! {
        _ = signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
        _ = async { while quit_rx.changed().await.is_ok() && !*quit_rx.borrow() {} } => {}
    }
    info!("Shutting down...");

    let _ = shutdown_tx.send(true);
    // Give attached clients a moment to receive the host_shutdown packet.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
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

    remove_pid_file_if_ours(&pid_path, pid);

    info!("uscreen daemon stopped");
    Ok(())
}
