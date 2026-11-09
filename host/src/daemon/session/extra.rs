//! Starting the pipeline of a second (third, ...) tablet

use super::*;

/// Allocate and initialize an independent streaming pipeline for an additional tablet instance.
///
/// Binds dedicated video/input ports, assigns a distinct EVDI card device node, and spawns
/// asynchronous capture, streaming, and uinput workers.
pub fn spawn_extra_session(
    t: &ExtraSessionTemplate,
    instance: u32,
    serial: &str,
    hw_id: String,
    transport: Transport,
) -> Option<ExtraSession> {
    let cards = vdisplay::evdi_cards();
    let mut cfg = t.cap_template.clone();
    cfg.instance = instance;

    let file_cfg = FileConfig::load();
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
            cfg.position = Position::parse_or_default(pos);
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
    let codec = capture::Codec::from_encoder(&cfg.encoder)
        .name()
        .to_string();

    cfg.card = cards.get(instance as usize).copied();
    if cfg.card.is_none() {
        warn!(
            "Device {} has no EVDI device of its own: {} exist, {} are wanted. \
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

    let (settings_tx, settings_rx) = watch::channel(EncoderSettings {
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
        restart_nonce: 0,
    });
    let settings_tx_keep = settings_tx.clone();
    let mut cap = CaptureManager::new(cfg.clone());
    let card_rx = cap.card_rx();
    let codec_config = cap.codec_config_arc();
    let latency = cap.latency_tracker();
    let relaunch = Arc::new(Notify::new());
    let stream_srv = StreamServer::new(
        StreamConfig {
            video_port,
            token: t.token.clone(),
        },
        codec_config,
        cap.idr_request_flag(),
    )
    .with_shutdown(t.shutdown_rx.clone());
    let input_srv = InputServer::new(
        InputConfig {
            port: input_port,
            instance,
            token: t.token.clone(),
            codec,
            virtual_width: cfg.width,
            virtual_height: cfg.height,
            pointer_handoff: FileConfig::load().pointer_handoff,
        },
        Some(settings_tx),
        latency,
        relaunch.clone(),
        card_rx.clone(),
        t.ping_notify.clone(),
    );

    let (tablet_tx, mut tablet_rx) = watch::channel(false);
    let (gate_tx, gate_rx) = watch::channel(false);
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut touchpad_rx = crate::config::options::options_rx(instance);
    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        let mut last = false;
        loop {
            let streams = touchpad_rx.borrow().input_mode.streams_video();
            let active = *tablet_rx.borrow() && streams;
            if active != last {
                last = active;
                let _ = gate_tx.send(active);
            }
            tokio::select! {
                r = tablet_rx.changed() => if r.is_err() { break },
                r = touchpad_rx.changed() => if r.is_err() { break },
            }
        }
    }));
    let (video_tx, _) = broadcast::channel(8);
    let video_rx = video_tx.subscribe();

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
        if let Err(e) = cap
            .stream_frames(video_tx, settings_rx, gate_rx, cap_stop_rx)
            .await
        {
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
        "Device slot {} ready: video port {}, input port {}{}",
        instance + 1,
        video_port,
        input_port,
        cfg.card
            .map(|c| format!(", EVDI card{}", c))
            .unwrap_or_default()
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
        last_relaunch: Instant::now() - Duration::from_secs(60),
        relaunch_wait: Duration::from_secs(5),
        relaunches: 0,
    })
}
