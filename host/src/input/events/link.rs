//! Messages about the link and settings rather than input: the tablet's resolution report (which
//! may reconfigure the virtual display), streaming settings pushed from the app, the legacy mode
//! message, foreground pings, and the preferred-transport switch.

use super::*;

/// The tablet's native resolution and physical size; may switch the virtual display to match.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_resolution(
    width: u32,
    height: u32,
    width_mm: u32,
    height_mm: u32,
    preferred_transport: Option<String>,
    version: Option<String>,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
    settings_tx: &Option<watch::Sender<EncoderSettings>>,
) {
    info!(
        "Tablet reports native resolution: {}x{} ({}x{} mm), version: {:?}, transport: {:?}",
        width, height, width_mm, height_mm, version, preferred_transport
    );
    if let Some(ref pref) = preferred_transport {
        let mut cfg = crate::config::FileConfig::load();
        let mut changed = false;
        for p in cfg.tablets.values_mut() {
            if p.preferred_transport.as_deref() != Some(pref) {
                p.preferred_transport = Some(pref.clone());
                changed = true;
            }
        }
        if changed {
            let _ = cfg.save();
        }
    }
    let Some(tx) = settings_tx else { return };
    if !crate::config::FileConfig::load().auto_resolution {
        info!("auto_resolution is off — keeping configured resolution");
        return;
    }
    if !(640..=8192).contains(&width) || !(480..=8192).contains(&height) {
        warn!("Ignoring implausible resolution {}x{}", width, height);
        return;
    }
    if (50..=1000).contains(&width_mm) && (50..=1000).contains(&height_mm) {
        if let Ok(mut guard) = uinput.lock() {
            guard.tablet_mm = (width_mm as f64, height_mm as f64);
        }
    }
    let mut new = tx.borrow().clone();
    // Reject nonsense physical sizes rather than baking them into an
    // EDID: a bad DPI makes the desktop come up at a absurd scale.
    let (mm_w, mm_h) = if (50..=1000).contains(&width_mm) && (50..=1000).contains(&height_mm) {
        (width_mm, height_mm)
    } else {
        (
            crate::display::edid::DEFAULT_WIDTH_MM,
            crate::display::edid::DEFAULT_HEIGHT_MM,
        )
    };
    if new.width != width || new.height != height || new.width_mm != mm_w || new.height_mm != mm_h {
        new.width = width;
        new.height = height;
        new.width_mm = mm_w;
        new.height_mm = mm_h;
        info!(
            "Auto-resolution: switching virtual display to {}x{} ({}x{} mm)",
            width, height, mm_w, mm_h
        );
        let _ = tx.send(new);
    }
}

/// Streaming settings pushed from the tablet's settings UI (clamped to the config's limits).
pub(super) fn handle_config(
    bitrate: Option<u32>,
    fps: Option<u32>,
    encoder: Option<String>,
    settings_tx: &Option<watch::Sender<EncoderSettings>>,
) {
    let Some(tx) = settings_tx else {
        warn!("Received config from tablet but live settings are disabled");
        return;
    };
    let mut new = tx.borrow().clone();
    if let Some(b) = bitrate {
        // Clamped to the same ceiling the config file uses: an
        // unclamped value here would be persisted and poison every
        // later run, which is exactly how installs ended up pinned at
        // 200 Mbps with seconds of queueing delay.
        new.bitrate = b.clamp(
            crate::config::MIN_BITRATE_KBPS,
            crate::config::MAX_BITRATE_KBPS,
        );
        if new.bitrate != b {
            warn!("Tablet asked for {} kbps — clamped to {}", b, new.bitrate);
        }
    }
    if let Some(f) = fps {
        new.fps = f.clamp(crate::config::MIN_FPS, crate::config::MAX_FPS);
    }
    if let Some(e) = encoder {
        new.encoder = e;
    }
    if *tx.borrow() != new {
        info!(
            "Tablet pushed settings: encoder={} {}kbps @{}fps",
            new.encoder, new.bitrate, new.fps
        );
        let _ = tx.send(new);
    }
}

/// Older apps ask for graphics-tablet mode with this message; it is stored as the input mode.
pub(super) fn handle_mode(instance: u32, pen_only: bool) {
    // Older apps ask for graphics-tablet mode with this message. It is now the device's
    // input mode, stored like any other setting.
    let mode = if pen_only { "stylus" } else { "display" };
    info!("Tablet asked for {} mode", mode);
    crate::config::options::set_from_device(instance, "input_mode", &serde_json::json!(mode));
}

/// The app came to the foreground: re-assert the adb reverse tunnels at once.
pub(super) fn handle_ping(ping_notify: &Arc<tokio::sync::Notify>) {
    info!("Received foreground ping from client — immediately re-asserting adb reverse tunnels");
    ping_notify.notify_waiters();
}

/// The app switched its preferred transport (auto, usb, wifi); remember it for every profile.
pub(super) fn handle_preferred_transport(transport: &str) {
    info!("Client reported preferred transport switch: {}", transport);
    let mut cfg = crate::config::FileConfig::load();
    let mut changed = false;
    for p in cfg.tablets.values_mut() {
        if p.preferred_transport.as_deref() != Some(transport) {
            p.preferred_transport = Some(transport.to_string());
            changed = true;
        }
    }
    if changed {
        let _ = cfg.save();
    }
}
