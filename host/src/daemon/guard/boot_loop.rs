//! The boot-loop guard: a state file that remembers whether the last run ended cleanly and decides whether the virtual display may start

use super::*;

/// Lifecycle state persisted by the boot-loop guard between daemon runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// A virtual monitor was created and has not yet proven healthy.
    DisplayUp,
    /// The display was up and the compositor stayed responsive for `HEALTHY_AFTER`.
    Healthy,
    /// The breaker fired: the compositor stopped answering while the display was up.
    Tripped,
    /// The display was removed on purpose, or the daemon shut down cleanly.
    Stopped,
}

/// What the guard writes to its state file: which boot, what state, since when, and how many consecutive bad runs (`strikes`) preceded this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardRecord {
    pub boot_id: String,
    pub state: RunState,
    pub since_unix: u64,
    #[serde(default)]
    pub strikes: u32,
}

/// Outcome of [`evaluate`]: start normally, or stay in safe mode (no virtual display).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Normal,
    /// Do not create a virtual monitor; the reason is for the log.
    Safe(String),
}

/// Decide, at daemon start, from what the previous run left behind.
pub fn evaluate(prev: Option<&GuardRecord>, boot_id: &str) -> Verdict {
    let Some(p) = prev else {
        return Verdict::Normal;
    };
    match p.state {
        RunState::DisplayUp if p.boot_id != boot_id => Verdict::Safe(format!(
            "the previous boot ended with the virtual display up and never proven healthy \
             (a desktop freeze followed by a hard reset looks exactly like this); strikes: {}",
            p.strikes + 1
        )),
        RunState::Tripped => Verdict::Safe(
            "the stall watchdog tripped last time: the compositor stopped answering while the \
             virtual display was up"
                .into(),
        ),
        _ => Verdict::Normal,
    }
}

/// Where the guard state file lives (`$XDG_STATE_HOME/uscreen/guard.json`, falling back to `~/.local/state`). It sits outside the runtime dir so it survives reboots, which is the whole point.
pub fn state_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("uscreen").join("guard.json")
}

/// The kernel's boot id, which changes on every boot (empty if unreadable).
pub(super) fn boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Seconds since the Unix epoch.
pub(super) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The saved guard record, `None` if it is missing or unreadable.
pub(super) fn load_record() -> Option<GuardRecord> {
    serde_json::from_str(&std::fs::read_to_string(state_path()).ok()?).ok()
}

/// Persist synchronously and durably: this file exists to survive a hard reset,
/// so it is fsynced and renamed into place.
pub(super) fn save_record(rec: &GuardRecord) {
    use std::io::Write;
    let path = state_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    let Ok(text) = serde_json::to_string(rec) else {
        return;
    };
    let ok = std::fs::File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        })
        .and_then(|_| std::fs::rename(&tmp, &path));
    if let Err(e) = ok {
        warn!("guard: could not persist {:?}: {}", path, e);
    }
}

/// `uscreen reset-guard`: forget the previous incident.
pub fn reset() -> std::io::Result<bool> {
    match std::fs::remove_file(state_path()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
