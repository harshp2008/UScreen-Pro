//! Multi-device session state machine, display pipeline orchestration, and lifecycle tracking.
//!
//! This module coordinates independent streaming pipelines for primary and secondary tablet
//! connections. Each active device is allocated an EVDI display device node, video encoder,
//! network streaming server, and uinput input receiver, isolated from other concurrent sessions.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Notify};
use tracing::{error, info, warn};

use crate::adb::{
    announce_transport, app_running, auto_provision_wifi, is_fake_serial, launch_app,
    on_tablet_connected, scan_adb_devices, setup_adb_forwarding, spawn_adb_tracker,
    teardown_adb_forwarding, DiscoveredDevice, Transport,
};
use crate::capture::{self, CaptureConfig, CaptureManager, EncoderSettings};
use crate::config::{FileConfig, Position};
use crate::input::{InputConfig, InputServer};
use crate::stream::{StreamConfig, StreamServer};
use crate::vdisplay;

/// Asynchronous control command dispatched across IPC or local threads to manipulate session state.
pub enum DaemonControlMsg {
    /// Temporarily suspend video capture, tearing down helper/ffmpeg processes to allow
    /// in-place APK updates without file locking or stream corruption.
    PauseDevice {
        serial: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    /// Re-establish display capture and input forwarding following a surgical update.
    ResumeDevice {
        serial: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
}

/// Baseline configuration template used to spawn secondary tablet sessions on demand.
///
/// Holds common daemon parameters (encoder, resolution defaults, network ports) that are
/// cloned and tailored when a secondary tablet connects.
pub struct ExtraSessionTemplate {
    /// Maximum number of concurrent tablet sessions allowed.
    pub max_tablets: u32,
    /// Base capture configuration template.
    pub cap_template: CaptureConfig,
    /// Base video port (slot 0 uses `video_port`, slot `i` uses `video_port + 2*i`).
    pub video_port: u16,
    /// Base input port (slot 0 uses `input_port`, slot `i` uses `input_port + 2*i`).
    pub input_port: u16,
    /// Shared session security token, if authentication is enforced.
    pub token: Option<String>,
    /// Global pen-only mode broadcast channel.
    pub mode_tx: watch::Sender<bool>,
    /// Daemon shutdown broadcast receiver.
    pub shutdown_rx: watch::Receiver<bool>,
    /// Foreground ping notification synchronization primitive.
    pub ping_notify: Arc<Notify>,
}

/// Active execution state and task handles for a secondary tablet display pipeline.
pub struct ExtraSession {
    /// Zero-based slot index (slot 1, slot 2, etc.).
    pub instance: u32,
    /// Persistent hardware identifier of the attached device.
    #[allow(dead_code)]
    pub hw_id: String,
    /// Currently active ADB endpoint serial (USB ID or IP:port).
    pub active_serial: String,
    /// Transport medium in active use.
    pub transport: Transport,
    /// Dedicated encoder settings watch channel for this display slot.
    pub settings_tx: watch::Sender<EncoderSettings>,
    /// Tablet attachment state watch channel.
    pub tablet_tx: watch::Sender<bool>,
    /// Re-launch trigger notification primitive.
    pub relaunch: Arc<Notify>,
    /// Stop signal sender for terminating background tasks.
    pub stop_tx: watch::Sender<bool>,
    /// Background join handles for capture, stream, input, and watchdog tasks.
    pub tasks: Vec<tokio::task::JoinHandle<()>>,
    /// Allocated TCP video streaming port.
    pub video_port: u16,
    /// Allocated TCP input forwarding port.
    pub input_port: u16,
    /// Timestamp of last APK relaunch attempt.
    pub last_relaunch: Instant,
    /// Current exponential backoff interval between relaunch attempts.
    pub relaunch_wait: Duration,
    /// Number of relaunch attempts initiated.
    pub relaunches: u32,
}

impl ExtraSession {
    /// Gracefully terminate all background streaming tasks and release EVDI card resources.
    pub fn stop_background(self) {
        tokio::spawn(async move {
            let _ = self.tablet_tx.send(false);
            let _ = self.stop_tx.send(true);
            tokio::time::sleep(Duration::from_millis(600)).await;
            for t in self.tasks {
                t.abort();
            }
        });
    }
}

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
        .muxer()
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
    );
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
        t.mode_tx.clone(),
        latency,
        relaunch.clone(),
        card_rx.clone(),
        t.ping_notify.clone(),
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

/// Apply remembered device configuration profile (resolution, refresh rate, bitrate, encoder).
///
/// Pre-configuring these parameters before EVDI display initialization avoids the multi-second
/// display flash and mode-switch pipeline rebuild that occurs if defaults are changed dynamically.
pub async fn apply_tablet_profile(serial: &str, settings_tx: &watch::Sender<EncoderSettings>) {
    let mut cfg = FileConfig::load();
    let Some(mut p) = cfg.tablets.get(serial).cloned() else {
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
    s.encoder = p
        .encoder
        .as_ref()
        .filter(|e| !e.trim().is_empty())
        .cloned()
        .unwrap_or(cfg.encoder.clone());
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

/// Central device supervision and session state machine loop.
///
/// Continuously monitors connected ADB endpoints, binds physical hardware devices to
/// display slots, executes seamless USB <-> Wi-Fi transport handoff, coordinates
/// surgical update pause/resume workflows, and enforces token-stamped app watchdog relaunching.
#[allow(clippy::too_many_arguments)]
pub async fn adb_monitor(
    video_port: u16,
    input_port: u16,
    auto_launch: bool,
    tablet_tx: watch::Sender<bool>,
    token: Option<String>,
    relaunch: Arc<Notify>,
    extra: ExtraSessionTemplate,
    wifi_address: String,
    settings_tx: watch::Sender<EncoderSettings>,
    serial_tx: watch::Sender<Option<String>>,
    mut control_rx: mpsc::Receiver<DaemonControlMsg>,
    ping_notify: Arc<Notify>,
) {
    let mut paused_serials: HashSet<String> = HashSet::new();
    let mut slot_0_hw: Option<String> = None;
    let mut slot_0_active_serial: String = String::new();
    let mut slot_0_transport: Transport = Transport::Usb;
    let mut slot_0_last_relaunch = Instant::now() - Duration::from_secs(60);
    let mut slot_0_relaunches: u32 = 0;
    let mut slot_0_relaunch_wait = Duration::from_secs(5);
    const RELAUNCH_WAIT_MAX: Duration = Duration::from_secs(600);

    let mut polls_since_check: u32 = 0;
    const APP_CHECK_EVERY: u32 = 2;
    let mut extras: HashMap<String, ExtraSession> = HashMap::new();
    let mut hw_cache: HashMap<String, String> = HashMap::new();
    let mut offline_since: HashMap<String, Instant> = HashMap::new();
    let mut track_rx = spawn_adb_tracker();

    if tokio::process::Command::new("adb")
        .arg("version")
        .output()
        .await
        .is_err()
    {
        error!("adb is not installed — devices cannot be found. Install android-tools (or adb) and restart.");
    }

    loop {
        let all_devices = scan_adb_devices(&mut hw_cache).await;

        // Targeted reconnect for offline devices without restarting host ADB server
        for dev in &all_devices {
            if dev.state == "offline" {
                let entry = offline_since
                    .entry(dev.serial.clone())
                    .or_insert_with(Instant::now);
                if entry.elapsed() >= Duration::from_secs(4) {
                    warn!(
                        "Device {} is offline ({:?}) — issuing targeted reconnect",
                        dev.serial,
                        entry.elapsed()
                    );
                    if dev.transport == Transport::Network {
                        let _ = tokio::process::Command::new("adb")
                            .args(["disconnect", &dev.serial])
                            .output()
                            .await;
                    } else {
                        let _ = tokio::process::Command::new("adb")
                            .args(["-s", &dev.serial, "reconnect"])
                            .output()
                            .await;
                    }
                    *entry = Instant::now();
                }
            } else {
                offline_since.remove(&dev.serial);
            }
        }

        // Group ready devices by persistent hardware ID, honoring configured transport preference
        let ready_devices: Vec<&DiscoveredDevice> =
            all_devices.iter().filter(|d| d.state == "device").collect();
        let mut physical_devices: HashMap<String, &DiscoveredDevice> = HashMap::new();
        let current_file_cfg = FileConfig::load();

        for dev in ready_devices {
            let pref = current_file_cfg
                .tablets
                .get(&dev.hw_id)
                .and_then(|p| p.preferred_transport.as_deref())
                .unwrap_or("auto");

            physical_devices
                .entry(dev.hw_id.clone())
                .and_modify(|existing| {
                    if pref == "wifi" {
                        if existing.transport != Transport::Network
                            && dev.transport == Transport::Network
                        {
                            *existing = dev;
                        }
                    } else if pref == "usb" {
                        if existing.transport != Transport::Usb && dev.transport == Transport::Usb {
                            *existing = dev;
                        }
                    } else if existing.transport != Transport::Usb
                        && dev.transport == Transport::Usb
                    {
                        *existing = dev;
                    }
                })
                .or_insert(dev);
        }

        // 1. Maintain Slot 0 Session with strict device isolation
        if let Some(ref s0_hw) = slot_0_hw.clone() {
            if paused_serials.contains(&slot_0_active_serial) {
                // Device paused for surgical update — retain state intact
            } else if let Some(best_dev) = physical_devices.get(s0_hw) {
                if !*tablet_tx.borrow() {
                    let _ = tablet_tx.send(true);
                }
                // Check for seamless USB <-> Wi-Fi transport handoff
                if best_dev.serial != slot_0_active_serial {
                    info!(
                        "[TRANSPORT-SWITCH] Seamless handoff: Slot 1 switched transport from {} ({}) to {} ({}) without display teardown",
                        slot_0_transport.label(), slot_0_active_serial,
                        best_dev.transport.label(), best_dev.serial
                    );
                    if !is_fake_serial(&slot_0_active_serial) {
                        teardown_adb_forwarding(&slot_0_active_serial, video_port, input_port)
                            .await;
                    }
                    if !is_fake_serial(&best_dev.serial) {
                        on_tablet_connected(
                            &best_dev.serial,
                            video_port,
                            input_port,
                            auto_launch,
                            token.as_deref(),
                        )
                        .await;
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
                        teardown_adb_forwarding(
                            &sess.active_serial,
                            sess.video_port,
                            sess.input_port,
                        )
                        .await;
                    }
                    if !is_fake_serial(&best_dev.serial) {
                        on_tablet_connected(
                            &best_dev.serial,
                            sess.video_port,
                            sess.input_port,
                            auto_launch,
                            token.as_deref(),
                        )
                        .await;
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
                info!(
                    "Device on Slot {} disconnected ({})",
                    sess.instance + 1,
                    sess.active_serial
                );
                if !is_fake_serial(&sess.active_serial) {
                    teardown_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port)
                        .await;
                }
                sess.stop_background();
            }
        }

        // 3. Assign newly attached devices to free slots without disturbing active sessions
        for (hw_id, dev) in &physical_devices {
            if paused_serials.contains(&dev.serial) {
                continue;
            }
            if slot_0_hw.as_ref() == Some(hw_id) || extras.contains_key(hw_id) {
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
                    on_tablet_connected(
                        &dev.serial,
                        video_port,
                        input_port,
                        auto_launch,
                        token.as_deref(),
                    )
                    .await;
                    if dev.transport == Transport::Usb {
                        auto_provision_wifi(&dev.serial).await;
                    }
                }
                let _ = tablet_tx.send(true);
                slot_0_hw = Some(hw_id.clone());
                slot_0_active_serial = dev.serial.clone();
                slot_0_transport = dev.transport;
                slot_0_relaunches = 0;
                slot_0_relaunch_wait = Duration::from_secs(5);
            } else if extra.max_tablets > 1 {
                let used: Vec<u32> = extras.values().map(|s| s.instance).collect();
                let Some(instance) = (1..extra.max_tablets).find(|i| !used.contains(i)) else {
                    warn!(
                        "Device {} attached but all {} slots are taken",
                        dev.serial, extra.max_tablets
                    );
                    continue;
                };
                info!(
                    "Device connected on Slot {} over {} ({})",
                    instance + 1,
                    dev.transport.label(),
                    dev.serial
                );
                let Some(sess) = spawn_extra_session(
                    &extra,
                    instance,
                    &dev.serial,
                    hw_id.clone(),
                    dev.transport,
                ) else {
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
                            let mut cfg = FileConfig::load();
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
                    on_tablet_connected(
                        &dev.serial,
                        sess.video_port,
                        sess.input_port,
                        auto_launch,
                        token.as_deref(),
                    )
                    .await;
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
            Some(_track_line) = track_rx.recv() => {}
            _ = ping_notify.notified() => {
                info!("Received foreground ping from device — re-asserting adb reverse tunnels immediately");
                if !slot_0_active_serial.is_empty() && !is_fake_serial(&slot_0_active_serial) && !paused_serials.contains(&slot_0_active_serial) {
                    let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                }
                for sess in extras.values() {
                    if !is_fake_serial(&sess.active_serial) && !paused_serials.contains(&sess.active_serial) {
                        let _ = setup_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                    }
                }
            }
            Some(msg) = control_rx.recv() => {
                match msg {
                    DaemonControlMsg::PauseDevice { serial, resp } => {
                        info!("Received surgical PAUSE request for device {}", serial);
                        paused_serials.insert(serial.clone());
                        if slot_0_active_serial == serial {
                            let _ = tablet_tx.send(false);
                            let fifo = capture::fifo_path_for(0);
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("evdi_helper .*--capture-fifo {}( |$)", fifo)])
                                .status().await;
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("ffmpeg.*{}", fifo)])
                                .status().await;
                            CaptureManager::disable_evdi_display(vdisplay::evdi_cards().first().copied()).await;
                            let _ = resp.send(Ok(()));
                        } else if let Some(sess) = extras.values_mut().find(|s| s.active_serial == serial) {
                            let _ = sess.tablet_tx.send(false);
                            let fifo = capture::fifo_path_for(sess.instance);
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("evdi_helper .*--capture-fifo {}( |$)", fifo)])
                                .status().await;
                            let _ = tokio::process::Command::new("pkill")
                                .args(["-f", &format!("ffmpeg.*{}", fifo)])
                                .status().await;
                            let cards = vdisplay::evdi_cards();
                            let card = cards.get(sess.instance as usize).copied();
                            CaptureManager::disable_evdi_display(card).await;
                            let _ = resp.send(Ok(()));
                        } else {
                            let fifo = capture::fifo_path_for(0);
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
            _ = tokio::time::sleep(Duration::from_secs(3)) => {
                polls_since_check += 1;
                if polls_since_check < APP_CHECK_EVERY {
                    continue;
                }
                polls_since_check = 0;

                // Background Wi-Fi Reconnection Watchdog
                let file_cfg = FileConfig::load();
                let mut targets = Vec::new();
                if !file_cfg.wifi_address.is_empty() {
                    targets.push(file_cfg.wifi_address.clone());
                }
                if !wifi_address.is_empty() && !targets.contains(&wifi_address) {
                    targets.push(wifi_address.clone());
                }
                for p in file_cfg.tablets.values() {
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
                            let _ = tokio::time::timeout(
                                Duration::from_secs(3),
                                tokio::process::Command::new("adb").args(["connect", &target]).output(),
                            ).await;
                        });
                    }
                }

                // App Running Watchdog for Slot 0
                if auto_launch && !slot_0_active_serial.is_empty() && !is_fake_serial(&slot_0_active_serial) && !paused_serials.contains(&slot_0_active_serial) {
                    match app_running(&slot_0_active_serial).await {
                        Some(true) => {
                            slot_0_relaunch_wait = Duration::from_secs(5);
                        }
                        Some(false) if slot_0_last_relaunch.elapsed() >= slot_0_relaunch_wait => {
                            slot_0_last_relaunch = Instant::now();
                            info!("UScreen app not running on Slot 1 ({}) — re-verifying port forwarding and launching it", slot_0_active_serial);
                            let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                            launch_app(&slot_0_active_serial, token.as_deref(), video_port, input_port).await;
                            slot_0_relaunch_wait = (slot_0_relaunch_wait * 2).min(RELAUNCH_WAIT_MAX);
                        }
                        _ => {}
                    }
                }

                // App Running Watchdog for Extra Sessions
                if auto_launch {
                    for sess in extras.values_mut() {
                        if is_fake_serial(&sess.active_serial) || paused_serials.contains(&sess.active_serial) { continue; }
                        match app_running(&sess.active_serial).await {
                            Some(true) => {
                                sess.relaunch_wait = Duration::from_secs(5);
                            }
                            Some(false) if sess.last_relaunch.elapsed() >= sess.relaunch_wait => {
                                sess.last_relaunch = Instant::now();
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
                if !slot_0_active_serial.is_empty()
                    && !paused_serials.contains(&slot_0_active_serial)
                    && slot_0_last_relaunch.elapsed() >= slot_0_relaunch_wait
                {
                    slot_0_last_relaunch = Instant::now();
                    slot_0_relaunches += 1;
                    if slot_0_relaunches == 3 {
                        warn!("A client keeps connecting without a valid token. Update the UScreen app on the device (1.1.0 or newer).");
                    }
                    info!("Delivering session token to Slot 1 app (attempt {}, next in {:?})", slot_0_relaunches, slot_0_relaunch_wait.min(RELAUNCH_WAIT_MAX));
                    let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                    launch_app(&slot_0_active_serial, token.as_deref(), video_port, input_port).await;
                    slot_0_relaunch_wait = (slot_0_relaunch_wait * 2).min(RELAUNCH_WAIT_MAX);
                }
            }
            _ = extra_relaunch => {
                for sess in extras.values_mut() {
                    if is_fake_serial(&sess.active_serial) || paused_serials.contains(&sess.active_serial) { continue; }
                    if sess.last_relaunch.elapsed() >= sess.relaunch_wait {
                        sess.last_relaunch = Instant::now();
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
