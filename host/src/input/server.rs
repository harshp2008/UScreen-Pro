//! The input WebSocket server: accepts a tablet's connection, authenticates it with the session
//! token, answers the handshake and dispatches every incoming message (settings, pings, mode
//! changes, input events) to the right place.

use super::*;

/// Loopback WebSocket server that receives a tablet's input and settings messages and injects them into the desktop through uinput.
pub struct InputServer {
    pub(super) config: InputConfig,
    pub(super) running: Arc<AtomicBool>,
    pub(super) settings_tx: Option<watch::Sender<EncoderSettings>>,
    pub(super) latency: crate::latency::LatencyTracker,
    /// Woken when a client fails to authenticate, so the daemon can push the
    /// token to the app again over adb (a manually launched app never got
    /// one).
    pub(super) relaunch: Arc<tokio::sync::Notify>,
    /// The EVDI card this tablet's helper opened, once known. The devices are
    /// mapped onto that card's connector, not onto "the first EVDI output" -
    /// with two tablets that would put both pens on one screen.
    pub(super) card_rx: watch::Receiver<Option<u32>>,
    pub(super) ping_notify: Arc<tokio::sync::Notify>,
}

impl InputServer {
    /// Wire the server to the daemon's shared channels.
    pub fn new(
        config: InputConfig,
        settings_tx: Option<watch::Sender<EncoderSettings>>,
        latency: crate::latency::LatencyTracker,
        relaunch: Arc<tokio::sync::Notify>,
        card_rx: watch::Receiver<Option<u32>>,
        ping_notify: Arc<tokio::sync::Notify>,
    ) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            settings_tx,
            latency,
            relaunch,
            card_rx,
            ping_notify,
        }
    }

    /// Accept tablet connections until the process exits.
    pub async fn run(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        // Loopback only — see the note in net/stream.rs. This socket injects real
        // mouse/pen/touch events into the desktop through uinput, so exposing
        // it on the network hands over control of the machine.
        let addr = format!("127.0.0.1:{}", self.config.port);

        let listener = TcpListener::bind(&addr)
            .await
            .context(format!("Failed to bind input server to {}", addr))?;

        info!("Input server on ws://{}", addr);
        crate::options::spawn_idle_task(self.config.instance);

        // Created once for the lifetime of the daemon, not per connection.
        // Recreating them on every reconnect gave the devices a fresh identity
        // each time, so KDE reran input configuration and the output mapping
        // below had nothing stable to attach to.
        //
        // Device creation sleeps to let udev settle, so it runs off the async
        // runtime rather than blocking a worker thread.
        let mut ident = DeviceIdentity::for_instance(self.config.instance);
        if !self.config.pointer_handoff {
            ident.device_count = 2;
        }
        let ident_bg = ident.clone();
        let pointer_handoff = self.config.pointer_handoff;
        let devices = tokio::task::spawn_blocking(move || {
            let touch = match UInputDevice::new_touch(&ident_bg.touch, ident_bg.product_touch) {
                Ok(dev) => Some(dev),
                Err(e) => {
                    warn!("No touch device: {}. Touch will be logged only.", e);
                    None
                }
            };
            let pen = match UInputDevice::new_pen(&ident_bg.pen, ident_bg.product_pen) {
                Ok(dev) => Some(dev),
                Err(e) => {
                    warn!("No pen device: {}. Pen will be logged only.", e);
                    None
                }
            };
            let pointer = if !pointer_handoff {
                info!("pointer_handoff is off — the pen will not move the desktop cursor");
                None
            } else {
                match UInputDevice::new_pointer(&ident_bg.pointer, ident_bg.product_pointer) {
                    Ok(dev) => Some(dev),
                    Err(e) => {
                        warn!(
                            "No pointer device: {}. The cursor will vanish when the pen lifts.",
                            e
                        );
                        None
                    }
                }
            };
            InjectDevices {
                touch,
                pen,
                pointer,
                mouse_name: ident_bg.mouse.clone(),
                mouse_product: ident_bg.product_mouse,
                pad_name: ident_bg.touchpad.clone(),
                pad_product: ident_bg.product_touchpad,
                keyboard_name: ident_bg.keyboard.clone(),
                keyboard_product: ident_bg.product_keyboard,
                ..InjectDevices::headless()
            }
        })
        .await
        .unwrap_or_else(|_| InjectDevices::headless());
        let uinput = Arc::new(std::sync::Mutex::new(devices));

        // Follow the mode for as long as the daemon runs. A switch has to move
        // the pen and touch devices onto the other output, and drop anything
        // held at the moment of the switch — a finger or pen tip that was down
        // would otherwise stay down on a screen that is no longer listening.
        {
            let mut mode_rx = crate::options::pen_only_rx(self.config.instance);
            let mut card_rx = self.card_rx.clone();
            let devices = uinput.clone();
            let ident_map = ident.clone();
            let initial = *mode_rx.borrow_and_update();
            let card0 = *card_rx.borrow_and_update();
            // RandR changes (Xorg re-numbering or moving the virtual output) re-run the mapping.
            let mut output_changes = crate::display::x11::output_changes();
            // The first mapping waits for the virtual display to be enabled
            // like every later one, so it runs in the background rather than
            // holding up the accept loop below.
            tokio::spawn(async move {
                if let Ok(mut guard) = devices.lock() {
                    guard.set_stylus_mode(initial);
                }
                map_and_note_area(initial, &ident_map, card0, &devices).await;
                loop {
                    tokio::select! {
                        r = mode_rx.changed() => { if r.is_err() { break; } }
                        r = card_rx.changed() => { if r.is_err() { break; } }
                        r = async {
                            match output_changes.as_mut() {
                                Some(rx) => rx.changed().await,
                                None => std::future::pending().await,
                            }
                        } => {
                            if r.is_err() {
                                output_changes = None;
                                continue;
                            }
                        }
                    }
                    let pen_only = *mode_rx.borrow();
                    let card = *card_rx.borrow();
                    if let Ok(mut guard) = devices.lock() {
                        guard.release_all();
                        guard.set_stylus_mode(pen_only);
                    }
                    // Leaving display mode tears the virtual output down and
                    // entering it brings the output back. map_devices_to_output
                    // waits for the output it needs to actually be enabled, so
                    // a mode change during the wait simply restarts it.
                    info!(
                        "Mode is now {} — remapping input devices{}",
                        if pen_only {
                            "pen-only"
                        } else {
                            "second screen"
                        },
                        card.map(|c| format!(" (card{})", c)).unwrap_or_default()
                    );
                    map_and_note_area(pen_only, &ident_map, card, &devices).await;
                }
            });
        }

        // Make the devices a mode needs as soon as it is entered (and at start), off the async runtime
        // because creating a device sleeps while udev settles.
        {
            let mut opts_rx = crate::options::options_rx(self.config.instance);
            let devices = uinput.clone();
            tokio::spawn(async move {
                loop {
                    let (mode, native) = {
                        let o = opts_rx.borrow_and_update();
                        (o.input_mode, o.native_gestures_effective())
                    };
                    let d = devices.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        if let Ok(mut guard) = d.lock() {
                            guard.ensure_devices_for(mode, native);
                        }
                    })
                    .await;
                    if opts_rx.changed().await.is_err() {
                        break;
                    }
                }
            });
        }

        let config = self.config.clone();
        let running = self.running.clone();

        loop {
            let accept = tokio::select! {
                res = listener.accept() => res,
                _ = async {
                    while running.load(Ordering::SeqCst) {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                } => break,
            };

            let (socket, peer) = match accept {
                Ok(s) => s,
                Err(e) => {
                    error!("Input accept failed: {}", e);
                    continue;
                }
            };

            info!("Input client: {}", peer);
            let cfg = config.clone();
            let settings = self.settings_tx.clone();
            let latency = self.latency.clone();
            let devices = uinput.clone();
            let relaunch = self.relaunch.clone();
            let ping_notify = self.ping_notify.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(
                    socket,
                    cfg,
                    settings,
                    latency,
                    devices,
                    relaunch,
                    ping_notify,
                )
                .await
                {
                    warn!("Input handler {}: {}", peer, e);
                }
            });
        }

        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_connection(
    raw_stream: tokio::net::TcpStream,
    config: InputConfig,
    settings_tx: Option<watch::Sender<EncoderSettings>>,
    latency: crate::latency::LatencyTracker,
    uinput: Arc<std::sync::Mutex<InjectDevices>>,
    relaunch: Arc<tokio::sync::Notify>,
    ping_notify: Arc<tokio::sync::Notify>,
) -> Result<()> {
    // Input events are a few hundred bytes. The library default of 64 MiB
    // per message is a memory bill nobody on this socket should be able to
    // run up.
    let ws_cfg = WebSocketConfig {
        max_message_size: Some(64 * 1024),
        max_frame_size: Some(64 * 1024),
        ..Default::default()
    };
    let ws_stream = accept_async_with_config(raw_stream, Some(ws_cfg))
        .await
        .context("WebSocket handshake failed")?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut mode_rx = crate::options::pen_only_rx(config.instance);
    // The stream settings as they are now, and a way to hear when they change.
    let mut settings_rx = settings_tx.as_ref().map(|t| t.subscribe());
    let current_codec = |tx: &Option<watch::Sender<EncoderSettings>>, fallback: &str| -> String {
        tx.as_ref()
            .map(|t| {
                crate::capture::Codec::from_encoder(&t.borrow().encoder)
                    .name()
                    .to_string()
            })
            .unwrap_or_else(|| fallback.to_string())
    };
    let current_settings = |tx: &Option<watch::Sender<EncoderSettings>>| {
        tx.as_ref().map(|t| {
            let s = t.borrow();
            (s.fps, s.bitrate, s.encoder.clone())
        })
    };
    let mut last_sent = current_settings(&settings_tx);
    // Per-device options (touchpad mode, immersive mode, battery saver, idle dimming). Sent right
    // after the greeting and again whenever they change, whoever changed them.
    let mut options_rx = crate::options::options_rx(config.instance);

    // Authenticate before anything else happens: no greeting, no events.
    if let Some(expected) = config.token.as_deref() {
        let first =
            tokio::time::timeout(std::time::Duration::from_secs(3), ws_receiver.next()).await;
        let ok = match first {
            Ok(Some(Ok(Message::Text(text)))) => matches!(
                serde_json::from_str::<InputEvent>(&text),
                Ok(InputEvent::Auth { token }) if crate::runtime::token_matches(expected, &token)
            ),
            _ => false,
        };
        if !ok {
            // The app reconnects every two seconds; after a handful of these
            // the log has made its point.
            static DROPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = DROPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 5 {
                warn!(
                    "Input client did not authenticate — dropped. Re-sending the token to the app."
                );
            } else if n == 5 {
                warn!("Further unauthenticated clients will be dropped quietly.");
            }
            // Most likely the app was started by hand and never received a
            // token. Launching it again over adb delivers one.
            relaunch.notify_one();
            let _ = ws_sender.send(Message::Close(None)).await;
            return Ok(());
        }
    }

    let resp = InputResponse {
        status: "connected".to_string(),
        width: config.virtual_width,
        height: config.virtual_height,
        codec: current_codec(&settings_tx, &config.codec),
        pen_only: *mode_rx.borrow_and_update(),
        fps: last_sent.as_ref().map(|(f, _, _)| *f),
        bitrate_kbps: last_sent.as_ref().map(|(_, b, _)| *b),
        host_version: Some(
            std::env::var("USCREEN_MOCK_HOST_VERSION")
                .unwrap_or_else(|_| env!("GIT_VERSION_STRING").to_string()),
        ),
    };

    ws_sender
        .send(Message::Text(serde_json::to_string(&resp)?))
        .await?;

    // Host-to-app notices (paused, settings updating, APK updating). A state that is still in
    // force when the app connects is replayed first.
    let (sticky, mut notice_rx) = crate::net::notices::subscribe(config.instance);
    if let Some(text) = sticky {
        ws_sender.send(Message::Text(text)).await?;
    }
    let first_options = options_rx.borrow_and_update().config_update();
    ws_sender
        .send(Message::Text(first_options.to_string()))
        .await?;

    loop {
        let msg = tokio::select! {
            notice = notice_rx.recv() => {
                match notice {
                    Ok(text) => {
                        if ws_sender.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
                continue;
            }
            changed = options_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let update = options_rx.borrow_and_update().config_update();
                if ws_sender.send(Message::Text(update.to_string())).await.is_err() {
                    break;
                }
                continue;
            }
            incoming = ws_receiver.next() => match incoming {
                Some(m) => m,
                None => break,
            },
            // The mode changed — here, from the GUI, or from the command line.
            // Whoever changed it, the tablet has to hear about it: it decides
            // from this whether to expect a video stream at all.
            changed = mode_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let pen_only = *mode_rx.borrow();
                let now = current_settings(&settings_tx);
                let resp = InputResponse {
                    status: "mode".to_string(),
                    width: config.virtual_width,
                    height: config.virtual_height,
                    codec: current_codec(&settings_tx, &config.codec),
                    pen_only,
                    fps: now.as_ref().map(|(f, _, _)| *f),
                    bitrate_kbps: now.as_ref().map(|(_, b, _)| *b),
                    host_version: Some(std::env::var("USCREEN_MOCK_HOST_VERSION").unwrap_or_else(|_| env!("GIT_VERSION_STRING").to_string())),
                };
                last_sent = now;
                if ws_sender
                    .send(Message::Text(serde_json::to_string(&resp)?))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            // The stream settings changed — from the GUI, the config file or
            // this very tablet. Tell it the numbers only when fps or bitrate
            // really differ from what it was last told: the settings also
            // change for reasons the tablet does not care about.
            changed = async {
                match settings_rx.as_mut() {
                    Some(rx) => rx.changed().await.is_ok(),
                    None => std::future::pending::<bool>().await,
                }
            } => {
                if !changed {
                    settings_rx = None;
                    continue;
                }
                let now = current_settings(&settings_tx);
                if now == last_sent {
                    continue;
                }
                let resp = InputResponse {
                    status: "settings".to_string(),
                    width: config.virtual_width,
                    height: config.virtual_height,
                    codec: current_codec(&settings_tx, &config.codec),
                    pen_only: *mode_rx.borrow(),
                    fps: now.as_ref().map(|(f, _, _)| *f),
                    bitrate_kbps: now.as_ref().map(|(_, b, _)| *b),
                    host_version: Some(std::env::var("USCREEN_MOCK_HOST_VERSION").unwrap_or_else(|_| env!("GIT_VERSION_STRING").to_string())),
                };
                last_sent = now;
                if ws_sender
                    .send(Message::Text(serde_json::to_string(&resp)?))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };

        match msg {
            Ok(Message::Text(text)) => match serde_json::from_str::<InputEvent>(&text) {
                Ok(event) => {
                    handle_event(
                        config.instance,
                        event,
                        &uinput,
                        &settings_tx,
                        &latency,
                        &ping_notify,
                    );
                }
                Err(e) => {
                    warn!("Invalid input: {} - {}", e, text);
                }
            },
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(Message::Ping(data)) => {
                let _ = ws_sender.send(Message::Pong(data)).await;
            }
            _ => {}
        }
    }

    // Release any stuck MT slots or pen proximity. The devices themselves now
    // outlive the connection, so without this the next client inherits a
    // phantom finger or a pen stuck in proximity — which shows up as
    // unstoppable scrolling.
    if let Ok(mut guard) = uinput.lock() {
        guard.release_all();
    }

    Ok(())
}
