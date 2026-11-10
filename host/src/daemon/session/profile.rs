//! Applying a tablet's saved profile (stream settings) when it connects

use super::*;

/// Apply remembered device configuration profile (resolution, refresh rate, bitrate, encoder).
///
/// Pre-configuring these parameters before EVDI display initialization avoids the multi-second
/// display flash and mode-switch pipeline rebuild that occurs if defaults are changed dynamically.
pub async fn apply_tablet_profile(serial: &str, settings_tx: &watch::Sender<EncoderSettings>) {
    apply_profile_keys(&[serial], settings_tx, false).await;
}

/// Recompute a slot's [`EncoderSettings`] from the saved config and publish them.
///
/// `keys` are the identifiers the device's profile may be stored under, in priority order. With
/// no stored profile the global defaults apply. When `force` is set the settings are published
/// even if identical, with a bumped `restart_nonce`, so the capture loop kills and respawns
/// both ffmpeg and evdi_helper rather than waiting for a value to differ.
pub(super) async fn apply_profile_keys(
    keys: &[&str],
    settings_tx: &watch::Sender<EncoderSettings>,
    force: bool,
) {
    let serial = keys.first().copied().unwrap_or_default();
    let mut cfg = FileConfig::load();
    let found = keys.iter().find_map(|k| cfg.tablets.get(*k).cloned());
    let Some(mut p) = found.or_else(|| force.then(Default::default)) else {
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
    if p.label.trim().is_empty() && !is_fake_serial(serial) && !force {
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
    if force {
        s.restart_nonce = before.restart_nonce.wrapping_add(1);
    }

    if before == s {
        return;
    }
    info!(
        "Device profile applied for {} ({}): {}x{}@{}fps, encoder: {}, bitrate: {}k, quality: {}, scale: {}, 10-bit: {}{}",
        p.display_name(serial),
        serial,
        s.width, s.height, s.fps,
        s.encoder,
        s.bitrate,
        s.quality,
        s.stream_scale,
        s.ten_bit,
        if force { " (forced restart)" } else { "" },
    );
    let _ = settings_tx.send(s);
}
