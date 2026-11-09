//! Helpers to find and signal a tablet's capture pipeline (FIFO, slot, signals)

use super::*;

/// FIFO path identifying the capture pipeline serving `serial`, matched against the slot-0
/// hardware id / active serial and every extra session.
pub(super) fn pipeline_fifo(
    serial: &str,
    slot_0_hw: Option<&str>,
    slot_0_active: &str,
    extras: &HashMap<String, ExtraSession>,
) -> Option<String> {
    if slot_0_hw == Some(serial) || (!slot_0_active.is_empty() && slot_0_active == serial) {
        return Some(capture::fifo_path_for(0));
    }
    extras
        .values()
        .find(|s| s.hw_id == serial || s.active_serial == serial)
        .map(|s| capture::fifo_path_for(s.instance))
}

/// `(slot instance, hardware serial, active adb serial)` of the session serving `serial`.
pub(super) fn slot_of(
    serial: &str,
    slot_0_hw: Option<&str>,
    slot_0_active: &str,
    extras: &HashMap<String, ExtraSession>,
) -> Option<(u32, String, String)> {
    if slot_0_hw == Some(serial) || (!slot_0_active.is_empty() && slot_0_active == serial) {
        return Some((
            0,
            slot_0_hw.unwrap_or(serial).to_string(),
            slot_0_active.to_string(),
        ));
    }
    extras
        .values()
        .find(|s| s.hw_id == serial || s.active_serial == serial)
        .map(|s| (s.instance, s.hw_id.clone(), s.active_serial.clone()))
}

/// Send `sig` to the ffmpeg and evdi_helper belonging to capture FIFO `fifo`.
pub(super) async fn signal_pipeline(sig: &str, fifo: &str) {
    for pattern in [
        format!("evdi_helper .*--capture-fifo {}( |$)", fifo),
        format!("ffmpeg.*{}", fifo),
    ] {
        let _ = tokio::process::Command::new("pkill")
            .args([&format!("-{sig}"), "-f", &pattern])
            .status()
            .await;
    }
}
