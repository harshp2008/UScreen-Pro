//! Per-slot state shared by the session supervisor, the encoder and the input socket.
//!
//! Two small registries live here:
//!
//! * **Notices** - JSON messages the host pushes to the Android app over the input WebSocket
//!   (`paused`, `resumed`, `settings_updating`, `settings_applied`, `updating`, `update_done`, `unpaired`).
//!   The app shows a full-screen overlay for them. A notice that describes a *state* is sticky:
//!   it is replayed to a client that connects while the state still holds, so a tablet that
//!   reconnects mid-pause still shows "Connection Paused by Host".
//! * **Liveness** - how many encoded frames the current ffmpeg of a slot has produced. A restart
//!   is only reported as finished once the new encoder has actually delivered a frame, not when
//!   the process was spawned.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

struct Slot {
    tx: broadcast::Sender<String>,
    sticky: Option<String>,
}

fn slots() -> &'static Mutex<HashMap<u32, Slot>> {
    static SLOTS: OnceLock<Mutex<HashMap<u32, Slot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn slot_entry(map: &mut HashMap<u32, Slot>, instance: u32) -> &mut Slot {
    map.entry(instance).or_insert_with(|| Slot {
        tx: broadcast::channel(16).0,
        sticky: None,
    })
}

/// Subscribe to a slot's notices. The returned string is the sticky notice to replay first.
pub fn subscribe(instance: u32) -> (Option<String>, broadcast::Receiver<String>) {
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    let slot = slot_entry(&mut map, instance);
    (slot.sticky.clone(), slot.tx.subscribe())
}

/// Enter a state: remember the message so late joiners get it, and broadcast it.
pub fn set(instance: u32, msg: Value) {
    let text = msg.to_string();
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    let slot = slot_entry(&mut map, instance);
    slot.sticky = Some(text.clone());
    let _ = slot.tx.send(text);
}

/// Leave a state: forget the sticky message and broadcast the closing one.
pub fn release(instance: u32, msg: Value) {
    let text = msg.to_string();
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    let slot = slot_entry(&mut map, instance);
    slot.sticky = None;
    let _ = slot.tx.send(text);
}

/// Drop any sticky state without telling the client (the device is gone or forgotten).
pub fn forget(instance: u32) {
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = map.get_mut(&instance) {
        slot.sticky = None;
    }
}

/// Notice: the host paused this device's display.
pub fn paused() -> Value {
    json!({ "type": "paused" })
}
/// Notice: the host resumed this device's display.
pub fn resumed() -> Value {
    json!({ "type": "resumed" })
}
/// Notice: new stream settings are being applied.
pub fn settings_updating() -> Value {
    json!({ "type": "settings_updating" })
}
/// Notice: the new stream settings are live.
pub fn settings_applied() -> Value {
    json!({ "type": "settings_applied" })
}
/// Notice: the tablet's app is about to be replaced; carries both versions so the overlay can show them.
pub fn updating(host: &str, device: &str) -> Value {
    json!({ "type": "updating", "host": host, "device": device })
}
/// Sent once when the user forgets the device in the GUI; the app then shows its "Unpaired by Host" banner.
pub fn unpaired() -> Value {
    json!({ "type": "unpaired" })
}
/// Notice: the app update finished.
pub fn update_done() -> Value {
    json!({ "type": "update_done" })
}

const MAX_SLOTS: usize = 8;
static GENERATION: [AtomicU64; MAX_SLOTS] = [const { AtomicU64::new(0) }; MAX_SLOTS];
static FRAMES: [AtomicU64; MAX_SLOTS] = [const { AtomicU64::new(0) }; MAX_SLOTS];

fn idx(instance: u32) -> usize {
    (instance as usize).min(MAX_SLOTS - 1)
}

/// Encoder generation of a slot; changes every time ffmpeg is (re)spawned.
pub fn generation(instance: u32) -> u64 {
    GENERATION[idx(instance)].load(Ordering::SeqCst)
}

/// Called when a slot's encoder output starts being read. Returns the new generation.
pub fn encoder_started(instance: u32) -> u64 {
    let i = idx(instance);
    FRAMES[i].store(0, Ordering::SeqCst);
    GENERATION[i].fetch_add(1, Ordering::SeqCst) + 1
}

/// Count one encoded frame, unless a newer encoder has already taken over the slot.
pub fn frame_emitted(instance: u32, generation: u64) {
    let i = idx(instance);
    if GENERATION[i].load(Ordering::SeqCst) == generation {
        FRAMES[i].fetch_add(1, Ordering::SeqCst);
    }
}

/// Wait until the encoder of `instance` is newer than `generation_before` and has produced a
/// frame. Returns how long that took, or `None` on timeout.
pub async fn wait_live(instance: u32, generation_before: u64, timeout: Duration) -> Option<Duration> {
    let start = Instant::now();
    let i = idx(instance);
    loop {
        if GENERATION[i].load(Ordering::SeqCst) > generation_before && FRAMES[i].load(Ordering::SeqCst) > 0 {
            return Some(start.elapsed());
        }
        if start.elapsed() >= timeout {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sticky_notice_is_replayed_until_released() {
        set(40, paused());
        let (sticky, _rx) = subscribe(40);
        assert_eq!(sticky.as_deref(), Some(r#"{"type":"paused"}"#));
        release(40, resumed());
        let (sticky, _rx) = subscribe(40);
        assert!(sticky.is_none());
    }

    #[tokio::test]
    async fn restart_is_live_only_after_a_new_encoder_delivers_a_frame() {
        let before = generation(41);
        assert!(wait_live(41, before, Duration::from_millis(120)).await.is_none());
        let gen = encoder_started(41);
        assert!(wait_live(41, before, Duration::from_millis(120)).await.is_none(), "spawned but silent");
        frame_emitted(41, gen);
        assert!(wait_live(41, before, Duration::from_millis(500)).await.is_some());
        // A frame from the previous encoder must not count for the new one.
        let newer = encoder_started(41);
        frame_emitted(41, gen);
        assert!(wait_live(41, gen, Duration::from_millis(120)).await.is_none());
        frame_emitted(41, newer);
        assert!(wait_live(41, gen, Duration::from_millis(500)).await.is_some());
    }
}
