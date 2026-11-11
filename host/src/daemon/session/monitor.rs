//! The adb monitor: the long-running loop that watches for tablets and drives every session's lifecycle

use super::*;

/// Central device supervision and session state machine loop.
///
/// Continuously monitors connected ADB endpoints, binds physical hardware devices to
/// display slots, executes seamless USB <-> Wi-Fi transport handoff, coordinates
/// surgical update pause/resume workflows, and enforces token-stamped app watchdog relaunching.
#[allow(clippy::too_many_arguments)]
pub async fn adb_monitor(
    video_port: u16,
    input_port: u16,
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
    // Devices whose pipeline is SIGSTOPped (user-facing Pause), keyed by the serial the GUI used.
    let mut frozen_serials: HashSet<String> = HashSet::new();
    // Devices already compared against the host's version, keyed by hardware serial with the
    // adb serial they were checked through. Leaving the adb list clears the entry, so the next
    // connect (USB or Wi-Fi) is checked again.
    let mut update_checked: HashMap<String, String> = HashMap::new();
    let mut warned_unpaired: HashSet<String> = HashSet::new();
    let mut slot_0_hw: Option<String> = None;
    let mut slot_0_active_serial: String = String::new();
    let mut slot_0_transport: Transport = Transport::Usb;
    let mut slot_0_last_relaunch = Instant::now() - Duration::from_secs(60);
    let mut slot_0_relaunches: u32 = 0;
    let mut slot_0_relaunch_wait = Duration::from_secs(5);
    /// Upper bound of the doubling wait between relaunches of the tablet app (10 minutes).
    const RELAUNCH_WAIT_MAX: Duration = Duration::from_secs(600);

    let mut polls_since_check: u32 = 0;
    /// The app on the tablet is checked every this many polls.
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
        let current_file_cfg = FileConfig::load();
        // Only paired devices (those with a [tablets.<serial>] entry) get a session. Anything
        // else is a "new device" that waits for the user to add it.
        let ready_devices: Vec<&DiscoveredDevice> = all_devices
            .iter()
            .filter(|d| d.state == "device")
            .filter(|d| {
                let paired = is_fake_serial(&d.serial) || current_file_cfg.is_paired(&d.hw_id, &d.serial);
                if !paired && warned_unpaired.insert(d.hw_id.clone()) {
                    info!(
                        "Device {} ({}) is connected but not paired; add it in the GUI (Add device) or set auto_pair_new_devices = true",
                        d.hw_id, d.serial
                    );
                }
                if paired {
                    warned_unpaired.remove(&d.hw_id);
                }
                paired
            })
            .collect();
        let mut physical_devices: HashMap<String, &DiscoveredDevice> = HashMap::new();

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
                crate::net::notices::forget(0);
                frozen_serials.retain(|f| *f != *s0_hw && *f != slot_0_active_serial);
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
                crate::net::notices::forget(sess.instance);
                frozen_serials.retain(|f| *f != hw_id && *f != sess.active_serial);
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
                crate::config::options::bind(0, &[hw_id.as_str(), dev.serial.as_str()]);
                if !is_fake_serial(&dev.serial) {
                    on_tablet_connected(&dev.serial, video_port, input_port, token.as_deref())
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
                crate::config::options::bind(instance, &[hw_id.as_str(), dev.serial.as_str()]);
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

        // 4. Version check for every paired device that just connected, USB or Wi-Fi, whether or
        //    not it holds a slot: a mismatch queues `adb install -r` (two at a time).
        update_checked.retain(|hw, _| physical_devices.contains_key(hw));
        for (hw_id, dev) in &physical_devices {
            if is_fake_serial(&dev.serial) || paused_serials.contains(&dev.serial) {
                continue;
            }
            if update_checked.get(hw_id) == Some(&dev.serial) {
                continue;
            }
            update_checked.insert(hw_id.clone(), dev.serial.clone());
            let instance =
                slot_of(hw_id, slot_0_hw.as_deref(), &slot_0_active_serial, &extras).map(|s| s.0);
            let req = crate::net::installer::Request {
                hw: hw_id.clone(),
                serial: dev.serial.clone(),
                instance,
            };
            tokio::spawn(async move {
                crate::net::installer::check_on_connect(req).await;
            });
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
                    DaemonControlMsg::FreezeDevice { serial, resp } => {
                        match pipeline_fifo(&serial, slot_0_hw.as_deref(), &slot_0_active_serial, &extras) {
                            Some(fifo) => {
                                info!("Freezing pipeline of {} (SIGSTOP ffmpeg + evdi_helper)", serial);
                                signal_pipeline("STOP", &fifo).await;
                                if let Some((inst, _, _)) = slot_of(&serial, slot_0_hw.as_deref(), &slot_0_active_serial, &extras) {
                                    crate::net::notices::set(inst, crate::net::notices::paused());
                                }
                                frozen_serials.insert(serial);
                                let _ = resp.send(Ok(()));
                            }
                            None => {
                                let _ = resp.send(Err(format!("Device {} not found in active sessions", serial)));
                            }
                        }
                    }
                    DaemonControlMsg::ThawDevice { serial, resp } => {
                        match pipeline_fifo(&serial, slot_0_hw.as_deref(), &slot_0_active_serial, &extras) {
                            Some(fifo) => {
                                info!("Thawing pipeline of {} (SIGCONT ffmpeg + evdi_helper)", serial);
                                signal_pipeline("CONT", &fifo).await;
                                if let Some((inst, _, _)) = slot_of(&serial, slot_0_hw.as_deref(), &slot_0_active_serial, &extras) {
                                    crate::net::notices::release(inst, crate::net::notices::resumed());
                                }
                                frozen_serials.remove(&serial);
                                let _ = resp.send(Ok(()));
                            }
                            None => {
                                frozen_serials.remove(&serial);
                                let _ = resp.send(Err(format!("Device {} not found in active sessions", serial)));
                            }
                        }
                    }
                    DaemonControlMsg::ListFrozen { resp } => {
                        let _ = resp.send(frozen_serials.iter().cloned().collect());
                    }
                    DaemonControlMsg::RestartSession { serial, resp } => {
                        let mut restarted: Vec<RestartedSlot> = Vec::new();
                        let want = |hw: Option<&str>, active: &str| match serial.as_deref() {
                            None => true,
                            Some(s) => hw == Some(s) || active == s,
                        };
                        if !slot_0_active_serial.is_empty() && want(slot_0_hw.as_deref(), &slot_0_active_serial) {
                            let hw = slot_0_hw.clone().unwrap_or_default();
                            if let Some(fifo) = pipeline_fifo(&slot_0_active_serial, slot_0_hw.as_deref(), &slot_0_active_serial, &extras) {
                                signal_pipeline("CONT", &fifo).await;
                            }
                            frozen_serials.retain(|f| f != &slot_0_active_serial && *f != hw && Some(f.as_str()) != serial.as_deref());
                            let generation_before = crate::net::notices::generation(0);
                            crate::net::notices::set(0, crate::net::notices::settings_updating());
                            apply_profile_keys(&[serial.as_deref().unwrap_or(&slot_0_active_serial), &slot_0_active_serial, &hw], &settings_tx, true).await;
                            if !is_fake_serial(&slot_0_active_serial) {
                                let _ = setup_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                            }
                            restarted.push(RestartedSlot {
                                serial: if hw.is_empty() { slot_0_active_serial.clone() } else { hw },
                                instance: 0,
                                generation_before,
                            });
                        }
                        let matches: Vec<(String, String, String, u32)> = extras
                            .values()
                            .filter(|s| want(Some(s.hw_id.as_str()), &s.active_serial))
                            .map(|s| (s.hw_id.clone(), s.active_serial.clone(), capture::fifo_path_for(s.instance), s.instance))
                            .collect();
                        for (hw, active, fifo, instance) in matches {
                            signal_pipeline("CONT", &fifo).await;
                            frozen_serials.retain(|f| *f != active && *f != hw && Some(f.as_str()) != serial.as_deref());
                            if let Some(sess) = extras.get(&hw) {
                                let generation_before = crate::net::notices::generation(instance);
                                crate::net::notices::set(instance, crate::net::notices::settings_updating());
                                apply_profile_keys(&[serial.as_deref().unwrap_or(&active), &active, &hw], &sess.settings_tx, true).await;
                                if !is_fake_serial(&active) {
                                    let _ = setup_adb_forwarding(&active, sess.video_port, sess.input_port).await;
                                }
                                restarted.push(RestartedSlot { serial: hw, instance, generation_before });
                            }
                        }
                        info!("Restart requested for {:?}: {} session(s) restarting", serial, restarted.len());
                        let _ = resp.send(Ok(restarted));
                    }
                    DaemonControlMsg::InstallApk { serial, resp } => {
                        let slot = slot_of(&serial, slot_0_hw.as_deref(), &slot_0_active_serial, &extras);
                        let target = match &slot {
                            Some((_, hw, active)) => Some((hw.clone(), active.clone())),
                            None => physical_devices
                                .values()
                                .find(|d| d.hw_id == serial || d.serial == serial)
                                .map(|d| (d.hw_id.clone(), d.serial.clone())),
                        };
                        let _ = resp.send(match target {
                            Some((hw, adb_serial)) => {
                                let queued = crate::net::installer::enqueue(crate::net::installer::Request {
                                    hw: hw.clone(),
                                    serial: adb_serial,
                                    instance: slot.map(|s| s.0),
                                });
                                Ok(if queued { format!("queued {}", hw) } else { format!("{} already queued", hw) })
                            }
                            None => Err(format!("Device {} is not connected", serial)),
                        });
                    }
                    DaemonControlMsg::ForgetDevice { serial, resp } => {
                        let mut stopped = Vec::new();
                        // Tell the app before its socket is torn down, then give the input task a
                        // moment to push the message out (the app shows "Unpaired by Host").
                        let notify_instance = if slot_0_hw.as_deref() == Some(serial.as_str()) || (!slot_0_active_serial.is_empty() && slot_0_active_serial == serial) {
                            Some(0)
                        } else {
                            extras.values().find(|s| s.hw_id == serial || s.active_serial == serial).map(|s| s.instance)
                        };
                        if let Some(inst) = notify_instance {
                            crate::net::notices::release(inst, crate::net::notices::unpaired());
                            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                        }
                        if slot_0_hw.as_deref() == Some(serial.as_str()) || (!slot_0_active_serial.is_empty() && slot_0_active_serial == serial) {
                            if !is_fake_serial(&slot_0_active_serial) {
                                teardown_adb_forwarding(&slot_0_active_serial, video_port, input_port).await;
                            }
                            let _ = tablet_tx.send(false);
                            let _ = serial_tx.send(None);
                            crate::net::notices::forget(0);
                            stopped.push("slot 1".to_string());
                            slot_0_hw = None;
                            slot_0_active_serial.clear();
                        } else if let Some(hw) = extras
                            .values()
                            .find(|s| s.hw_id == serial || s.active_serial == serial)
                            .map(|s| s.hw_id.clone())
                        {
                            if let Some(sess) = extras.remove(&hw) {
                                if !is_fake_serial(&sess.active_serial) {
                                    teardown_adb_forwarding(&sess.active_serial, sess.video_port, sess.input_port).await;
                                }
                                crate::net::notices::forget(sess.instance);
                                stopped.push(format!("slot {}", sess.instance + 1));
                                sess.stop_background();
                            }
                        }
                        frozen_serials.retain(|f| *f != serial);
                        paused_serials.retain(|f| *f != serial);
                        update_checked.remove(&serial);
                        hw_cache.retain(|k, v| *k != serial && *v != serial);
                        info!("Forgot device {}: stopped {:?}", serial, stopped);
                        let _ = resp.send(Ok(if stopped.is_empty() { "no active session".to_string() } else { format!("stopped {}", stopped.join(", ")) }));
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
                if !slot_0_active_serial.is_empty() && !is_fake_serial(&slot_0_active_serial) && !paused_serials.contains(&slot_0_active_serial) && may_open_app(&slot_0_active_serial).await {
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
                {
                    for sess in extras.values_mut() {
                        if is_fake_serial(&sess.active_serial) || paused_serials.contains(&sess.active_serial) || !may_open_app(&sess.active_serial).await { continue; }
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
