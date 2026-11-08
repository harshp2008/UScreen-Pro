//! APK installation queue.
//!
//! Every install, whether triggered by the automatic version check when a paired device
//! connects or by the GUI's reinstall button, goes through one queue. At most
//! [`MAX_CONCURRENT`] installs run at once so several tablets do not saturate the USB bus; the
//! rest wait in `queued` and start as soon as a slot frees up. The state of every job is
//! readable through [`snapshot_json`] (control command `INSTALLS`), which is what the GUI shows
//! on the device cards.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{info, warn};

/// How many APK installs may run at once; more than this saturates adb and slows every install.
pub const MAX_CONCURRENT: usize = 2;
const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);
/// A finished job stays visible this long so the GUI's 3 s poll is guaranteed to see it.
const DONE_VISIBLE: Duration = Duration::from_secs(20);
const FAILED_VISIBLE: Duration = Duration::from_secs(120);
/// An automatic update that did not fix the mismatch is not retried for this long, so a stale
/// APK on disk can never put a device into an install loop.
const AUTO_COOLDOWN: Duration = Duration::from_secs(600);
/// Time the "Updating App..." overlay gets on the tablet before adb replaces the package.
const OVERLAY_LEAD: Duration = Duration::from_millis(1500);

/// Lifecycle of one APK install job as shown in the GUI.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Queued,
    Installing,
    Done,
    Failed,
}

impl State {
    fn label(&self) -> &'static str {
        match self {
            State::Queued => "queued",
            State::Installing => "installing",
            State::Done => "done",
            State::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
struct Job {
    state: State,
    detail: String,
    since: Instant,
}

fn jobs() -> &'static Mutex<HashMap<String, Job>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Job>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn semaphore() -> &'static Semaphore {
    static SEM: OnceLock<Semaphore> = OnceLock::new();
    SEM.get_or_init(|| Semaphore::new(MAX_CONCURRENT))
}

static RUNNING: AtomicUsize = AtomicUsize::new(0);

fn set_state(key: &str, state: State, detail: impl Into<String>) {
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    map.insert(
        key.to_string(),
        Job { state, detail: detail.into(), since: Instant::now() },
    );
}

fn is_active(key: &str) -> bool {
    let map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    map.get(key)
        .is_some_and(|j| matches!(j.state, State::Queued | State::Installing))
}

/// JSON object `{ "<device key>": { "state": "...", "detail": "..." } }` of every job that is
/// queued, running or recently finished. Expired entries are dropped.
pub fn snapshot_json() -> String {
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, j| match j.state {
        State::Done => j.since.elapsed() < DONE_VISIBLE,
        State::Failed => j.since.elapsed() < FAILED_VISIBLE,
        _ => true,
    });
    let obj: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, j)| {
            (
                k.clone(),
                serde_json::json!({ "state": j.state.label(), "detail": j.detail }),
            )
        })
        .collect();
    serde_json::Value::Object(obj).to_string()
}

/// Run `work` once a queue slot is free. The job is `queued` until then. Returns `false`
/// without doing anything when the device already has an active job.
pub fn spawn_limited<F, Fut>(key: String, sem: &'static Semaphore, work: F) -> bool
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<String, String>> + Send + 'static,
{
    if is_active(&key) {
        return false;
    }
    set_state(&key, State::Queued, "waiting for a free install slot");
    info!("Install queue: {} queued ({} running)", key, RUNNING.load(Ordering::SeqCst));
    tokio::spawn(async move {
        let _permit = sem.acquire().await.expect("install semaphore closed");
        let running = RUNNING.fetch_add(1, Ordering::SeqCst) + 1;
        set_state(&key, State::Installing, "installing");
        info!("Install queue: {} started ({}/{} slots busy)", key, running, MAX_CONCURRENT);
        let result = work().await;
        RUNNING.fetch_sub(1, Ordering::SeqCst);
        match result {
            Ok(msg) => {
                info!("Install queue: {} finished: {}", key, msg);
                set_state(&key, State::Done, msg);
            }
            Err(e) => {
                warn!("Install queue: {} failed: {}", key, e);
                set_state(&key, State::Failed, e);
            }
        }
    });
    true
}

/// Candidate locations of the built APK; the most recently modified one wins so a stale copy
/// in `~/.local/share/uscreen` can never shadow a fresh build.
pub fn find_apk() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let mut candidates = vec![
        home.join(".local/share/uscreen/app-debug.apk"),
        PathBuf::from("/usr/share/uscreen/app-debug.apk"),
        PathBuf::from("/usr/local/share/uscreen/app-debug.apk"),
    ];
    if let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) {
        for dir in exe.ancestors().skip(1).take(5) {
            candidates.push(dir.join("dist/app-debug.apk"));
            candidates.push(dir.join("android/app/build/outputs/apk/debug/app-debug.apk"));
        }
    }
    candidates
        .into_iter()
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((modified, p))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

/// `versionName` of the UScreen app installed on `serial`, if any.
pub async fn installed_version(serial: &str) -> Option<String> {
    let out = tokio::time::timeout(
        Duration::from_secs(8),
        Command::new("adb")
            .args(["-s", serial, "shell", "dumpsys", "package", "com.uscreen"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    parse_version_name(&String::from_utf8_lossy(&out.stdout))
}

fn parse_version_name(dumpsys: &str) -> Option<String> {
    dumpsys
        .lines()
        .find_map(|l| l.trim().strip_prefix("versionName="))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Version stamp of this host binary, compared against the app's reported version to decide whether the tablet needs an update.
pub fn host_version() -> &'static str {
    env!("GIT_VERSION_STRING")
}

async fn adb_install(serial: &str, apk: &std::path::Path) -> Result<(), String> {
    for downgrade in [false, true] {
        let mut args = vec!["-s", serial, "install", "-r"];
        if downgrade {
            args.push("-d");
        }
        let apk_str = apk.to_string_lossy().into_owned();
        args.push(&apk_str);
        let out = tokio::time::timeout(
            INSTALL_TIMEOUT,
            Command::new("adb").args(&args).kill_on_drop(true).output(),
        )
        .await
        .map_err(|_| "adb install timed out".to_string())?
        .map_err(|e| format!("could not run adb: {e}"))?;
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if text.contains("Success") {
            return Ok(());
        }
        if !downgrade && text.contains("INSTALL_FAILED_VERSION_DOWNGRADE") {
            continue;
        }
        return Err(format!("adb install: {}", text.trim().lines().last().unwrap_or("no output")));
    }
    Err("adb install failed".into())
}

/// What the install of one device needs to know about its slot.
pub struct Request {
    /// Key the GUI looks the job up by: the device's hardware serial.
    pub hw: String,
    /// adb serial to install through (`ip:port` or USB serial).
    pub serial: String,
    /// Slot whose tablet shows the "Updating App..." overlay, when the device has one.
    pub instance: Option<u32>,
}

/// Queue an install. Returns `false` when this device already has one queued or running.
pub fn enqueue(req: Request) -> bool {
    let Some(apk) = find_apk() else {
        set_state(&req.hw, State::Failed, "app-debug.apk not found; run ./build-all.sh");
        return true;
    };
    let Request { hw, serial, instance } = req;
    spawn_limited(hw.clone(), semaphore(), move || async move {
        let device_version = installed_version(&serial).await.unwrap_or_else(|| "not installed".into());
        let host = host_version();
        if let Some(i) = instance {
            crate::notices::set(i, crate::notices::updating(host, &device_version));
            tokio::time::sleep(OVERLAY_LEAD).await;
        }
        info!("Installing {} on {} (device has {}, host is {})", apk.display(), serial, device_version, host);
        let result = adb_install(&serial, &apk).await;
        if let Some(i) = instance {
            crate::notices::release(i, crate::notices::update_done());
        }
        result?;
        let now = installed_version(&serial).await.unwrap_or_default();
        if now == host {
            Ok(format!("updated {device_version} -> {now}"))
        } else {
            Err(format!(
                "installed, but the device reports version '{now}' while the host is '{host}'; rebuild with ./build-all.sh so the APK matches"
            ))
        }
    })
}

fn last_auto() -> &'static Mutex<HashMap<String, Instant>> {
    static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Run when a paired device connects: compare the installed version with the host's and queue
/// an update on mismatch. Returns the versions compared, for the log.
pub async fn check_on_connect(req: Request) -> Option<(String, String)> {
    let host = host_version().to_string();
    let device = installed_version(&req.serial).await;
    let shown = device.clone().unwrap_or_else(|| "not installed".into());
    if device.as_deref() == Some(host.as_str()) {
        info!("APK check {}: device {} matches host {}", req.hw, shown, host);
        return Some((shown, host));
    }
    {
        let mut map = last_auto().lock().unwrap_or_else(|e| e.into_inner());
        if map.get(&req.hw).is_some_and(|t| t.elapsed() < AUTO_COOLDOWN) {
            warn!("APK check {}: device {} != host {}, but an automatic update ran recently; not retrying", req.hw, shown, host);
            return Some((shown, host));
        }
        map.insert(req.hw.clone(), Instant::now());
    }
    info!("APK check {}: device {} != host {} -> queueing adb install -r", req.hw, shown, host);
    enqueue(req);
    Some((shown, host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[test]
    fn reads_version_name_from_dumpsys() {
        let text = "Packages:\n  Package [com.uscreen] (abc):\n    versionCode=22 minSdk=29\n    versionName=82b9705-dirty\n    splits=[base]\n";
        assert_eq!(parse_version_name(text).as_deref(), Some("82b9705-dirty"));
        assert_eq!(parse_version_name("Unable to find package: com.uscreen"), None);
    }

    /// Three devices need an update: two run together, the third waits until one finishes.
    #[tokio::test]
    async fn at_most_two_installs_run_and_the_third_waits() {
        static SEM: OnceLock<Semaphore> = OnceLock::new();
        let sem: &'static Semaphore = SEM.get_or_init(|| Semaphore::new(MAX_CONCURRENT));
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        for key in ["test-a", "test-b", "test-c"] {
            let (live, peak) = (live.clone(), peak.clone());
            assert!(spawn_limited(key.to_string(), sem, move || async move {
                let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(400)).await;
                live.fetch_sub(1, Ordering::SeqCst);
                Ok("ok".into())
            }));
        }
        // A duplicate request for a device that is already queued is refused.
        assert!(!spawn_limited("test-a".into(), sem, || async { Ok(String::new()) }));

        tokio::time::sleep(Duration::from_millis(150)).await;
        let snap: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        println!("t=150ms {snap}");
        let states: Vec<&str> = ["test-a", "test-b", "test-c"]
            .iter()
            .map(|k| snap[k]["state"].as_str().unwrap())
            .collect();
        assert_eq!(states.iter().filter(|s| **s == "installing").count(), 2);
        assert_eq!(states.iter().filter(|s| **s == "queued").count(), 1);

        tokio::time::sleep(Duration::from_millis(400)).await;
        let snap: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        println!("t=550ms {snap}");
        assert_eq!(snap["test-c"]["state"], "installing", "third starts once a slot frees");

        tokio::time::sleep(Duration::from_millis(500)).await;
        let snap: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        println!("t=1050ms {snap}");
        assert!(["test-a", "test-b", "test-c"].iter().all(|k| snap[k]["state"] == "done"));
        assert_eq!(peak.load(Ordering::SeqCst), 2, "never more than two at once");
    }
}
