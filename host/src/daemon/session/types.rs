//! The control messages and the records describing extra (non-primary) tablet sessions

use super::*;

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
    /// User-facing pause: SIGSTOP the ffmpeg and evdi_helper processes of a device, leaving
    /// every pipe, socket and the virtual display intact so SIGCONT continues instantly.
    FreezeDevice {
        serial: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    /// Undo [`DaemonControlMsg::FreezeDevice`] with SIGCONT.
    ThawDevice {
        serial: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    /// Re-read the saved config for one device (or every device when `serial` is `None`), then
    /// kill and respawn its ffmpeg and evdi_helper processes with the resulting arguments.
    RestartSession {
        serial: Option<String>,
        resp: oneshot::Sender<Result<Vec<RestartedSlot>, String>>,
    },
    /// Queue an APK install (at most two run at once) on the device with this serial.
    InstallApk {
        serial: String,
        resp: oneshot::Sender<Result<String, String>>,
    },
    /// The device was forgotten: stop its session now and drop everything held for it.
    ForgetDevice {
        serial: String,
        resp: oneshot::Sender<Result<String, String>>,
    },
    /// Serials currently frozen by [`DaemonControlMsg::FreezeDevice`].
    ListFrozen { resp: oneshot::Sender<Vec<String>> },
}

/// One pipeline that [`DaemonControlMsg::RestartSession`] respawned.
#[derive(Clone, Debug)]
pub struct RestartedSlot {
    /// Hardware serial (the GUI's key) or, failing that, the active adb serial.
    pub serial: String,
    pub instance: u32,
    /// Encoder generation before the restart; the restart is live once a newer one has a frame.
    pub generation_before: u64,
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
