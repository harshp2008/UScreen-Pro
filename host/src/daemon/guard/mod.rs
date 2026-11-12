//! Compositor safety: never let the virtual monitor be what stalls the desktop.
//!
//! See `experiments_and_findings/56_evdi_mutter_freeze_rca.md` for the evidence.
//! Four independent layers, each useful on its own:
//!
//! 1. **Readiness gate** - the virtual monitor is not created until the compositor
//!    is up *and idle*: GNOME Shell owns its bus name, the session manager reports
//!    `SessionRunning`, the shell process is a few seconds old, and its main loop
//!    answers `DisplayConfig.GetCurrentState` quickly several times in a row.
//!    (A plain `Peer.Ping` would not do: GDBus answers it on a worker thread, so it
//!    keeps working while the shell's main loop is wedged. `GetCurrentState` is
//!    served from the main loop, which is exactly what a stall starves.)
//! 2. **Hotplug limiter** - every helper start/stop is a monitor hotplug that the
//!    compositor must fully process (new GPU renderer, layout, docks, input
//!    mapping). They are spaced out and capped per minute.
//! 3. **Stall watchdog** - while the virtual monitor exists, the same probe runs
//!    every second. Consecutive stalls trip a breaker: the monitor is removed
//!    (helper stopped), and re-creation backs off, then latches off.
//! 4. **Boot-loop guard** - a small state file remembers whether the previous run
//!    ever got the display to a healthy state. If the machine died with the
//!    display up (or the breaker tripped), the next start comes up in *safe mode*
//!    and does not create a virtual monitor until `uscreen reset-guard`.
//!
//! Layout:
//! * `boot_loop` - the boot-loop guard: a state file that remembers whether the last run ended cleanly and decides whether the virtual display may start
//! * `detectors` - the pure detectors: the hotplug rate limiter and the compositor stall detector
//! * `probes` - compositor probes over D-Bus and the wait until the compositor is ready

mod boot_loop;
mod detectors;
mod probes;
pub use boot_loop::*;
pub use detectors::*;
pub use probes::*;

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tracing::{info, warn};

// ---- tunables -------------------------------------------------------------

/// A probe slower than this counts as "not idle" for the readiness gate.
const READY_PROBE_MAX: Duration = Duration::from_millis(500);
/// Consecutive fast probes the gate wants before declaring the shell idle.
const READY_PROBES_NEEDED: u32 = 2;
/// Pause between two readiness probes of the shell.
const READY_PROBE_GAP: Duration = Duration::from_millis(250);
/// Never proceed on a sluggish-but-answering shell before this long.
const READY_SLOW_FALLBACK: Duration = Duration::from_secs(180);

/// Minimum spacing between two virtual-monitor topology changes.
const HOTPLUG_MIN_GAP: Duration = Duration::from_secs(3);
/// More than this many changes inside `HOTPLUG_WINDOW` triggers a cool-down.
const HOTPLUG_MAX_PER_WINDOW: usize = 4;
/// Sliding window in which `HOTPLUG_MAX_PER_WINDOW` topology changes are allowed.
const HOTPLUG_WINDOW: Duration = Duration::from_secs(60);
/// How long hotplugs are held back once the window limit is exceeded.
const HOTPLUG_COOLDOWN: Duration = Duration::from_secs(30);

/// How often the watchdog samples the compositor's responsiveness.
const WATCHDOG_PERIOD: Duration = Duration::from_secs(1);
/// A probe that takes longer than this (or times out) is a stall sample.
const STALL_PROBE: Duration = Duration::from_millis(2000);
/// A probe that has not answered after this long is abandoned and counted as a stall sample.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Consecutive stall samples that trip the breaker.
const STALLS_TO_TRIP: u32 = 3;
/// Healthy for this long with the display up => the run is "healthy".
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
/// Cool-down after the n-th trip; the third trip latches the display off.
const TRIP_BACKOFF: [Duration; 2] = [Duration::from_secs(60), Duration::from_secs(300)];

// ---- boot-loop guard (pure logic + state file) -----------------------------

// ---- hotplug limiter (pure) -------------------------------------------------

// ---- stall detector (pure) ---------------------------------------------------

// ---- compositor probes --------------------------------------------------------

// ---- the guard handed to the capture manager ---------------------------------

/// Answer to "may I create the virtual display now?" given to the capture manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Go,
    /// Safe mode / latched breaker: do not create the display.
    Refused,
    Shutdown,
}

/// Runtime face of the safety system: combines the persisted boot verdict, the hotplug limiter and the stall-breaker latch, and tells the capture manager when it may bring the display up.
pub struct Guard {
    verdict: Verdict,
    limiter: HotplugLimiter,
    ready_once: bool,
    trips: usize,
    latched: bool,
    resume_after: Option<Instant>,
    record: GuardRecord,
}

impl Guard {
    /// Load the previous run's record, judge it, and stamp a fresh record for this run.
    pub fn new() -> Self {
        let id = boot_id();
        let prev = load_record();
        let verdict = evaluate(prev.as_ref(), &id);
        let strikes = match (&verdict, &prev) {
            (Verdict::Safe(_), Some(p)) => p.strikes + 1,
            _ => 0,
        };
        if let Verdict::Safe(why) = &verdict {
            warn!("guard: SAFE MODE - {}", why);
            warn!("guard: no virtual display will be created. Run `uscreen reset-guard` and restart the daemon to re-enable it");
        }
        let record = GuardRecord {
            boot_id: id,
            state: RunState::Stopped,
            since_unix: now_unix(),
            strikes,
        };
        // A safe verdict must survive until the user resets it, so do not
        // overwrite the evidence with "stopped" here.
        if verdict == Verdict::Normal {
            save_record(&record);
        }
        Self {
            verdict,
            limiter: HotplugLimiter::default(),
            ready_once: false,
            trips: 0,
            latched: false,
            resume_after: None,
            record,
        }
    }

    /// Record a new run state and persist it.
    fn set_state(&mut self, state: RunState) {
        self.record.state = state;
        self.record.since_unix = now_unix();
        save_record(&self.record);
    }

    /// Call before starting the helper (= creating/changing the virtual monitor).
    pub async fn admit(&mut self, shutdown: &mut watch::Receiver<bool>) -> Admission {
        if self.latched || matches!(self.verdict, Verdict::Safe(_)) {
            return Admission::Refused;
        }
        if let Some(until) = self.resume_after {
            let wait = until.saturating_duration_since(Instant::now());
            if !wait.is_zero() {
                info!(
                    "guard: breaker open - waiting {}s before trying the display again",
                    wait.as_secs()
                );
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = shutdown.changed() => return Admission::Shutdown,
                }
            }
            self.resume_after = None;
            self.ready_once = false; // re-prove the compositor after a stall
        }
        if !self.ready_once {
            if !wait_compositor_ready(shutdown).await {
                return Admission::Shutdown;
            }
            self.ready_once = true;
        }
        let wait = self.limiter.delay(Instant::now());
        if !wait.is_zero() {
            info!(
                "guard: spacing out display hotplugs - waiting {}ms",
                wait.as_millis()
            );
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = shutdown.changed() => return Admission::Shutdown,
            }
        }
        self.limiter.record(Instant::now());
        Admission::Go
    }

    /// The helper is now running: the virtual monitor exists.
    pub fn display_up(&mut self) {
        self.set_state(RunState::DisplayUp);
    }

    /// The virtual monitor was removed on purpose (or the daemon is exiting).
    pub fn display_down(&mut self) {
        self.limiter.record(Instant::now());
        if self.record.state != RunState::Tripped {
            self.set_state(RunState::Stopped);
        }
    }

    /// Promote this run from `DisplayUp` to `Healthy` once the display has stayed stable for the grace period.
    pub fn healthy(&mut self) {
        if self.record.state == RunState::DisplayUp {
            info!(
                "guard: virtual display has been stable for {}s - marking this run healthy",
                HEALTHY_AFTER.as_secs()
            );
            self.record.strikes = 0;
            self.set_state(RunState::Healthy);
        }
    }

    /// The watchdog fired. Records the trip and returns how long to stay off
    /// (`None` = latched off for the rest of the session).
    pub fn tripped(&mut self) -> Option<Duration> {
        self.set_state(RunState::Tripped);
        let backoff = TRIP_BACKOFF.get(self.trips).copied();
        self.trips += 1;
        match backoff {
            Some(d) => {
                self.resume_after = Some(Instant::now() + d);
                warn!("guard: compositor stalled with the virtual display up - display removed; retry in {}s", d.as_secs());
            }
            None => {
                self.latched = true;
                warn!("guard: compositor stalled again - virtual display disabled until `uscreen reset-guard` + daemon restart");
            }
        }
        backoff
    }
}

/// Spawn the stall watchdog. It only probes while `active` is true. The
/// returned receiver carries a counter that increments on every trip, and a
/// second one pulses when the display has been healthy for `HEALTHY_AFTER`.
pub fn spawn_watchdog(
    mut active: watch::Receiver<bool>,
) -> (watch::Receiver<u64>, watch::Receiver<u64>) {
    let (trip_tx, trip_rx) = watch::channel(0u64);
    let (ok_tx, ok_rx) = watch::channel(0u64);
    if !is_gnome_wayland() {
        // Only GNOME has a probe worth trusting here; leave the channels idle.
        return (trip_rx, ok_rx);
    }
    tokio::spawn(async move {
        let mut conn: Option<zbus::Connection> = None;
        let (mut trips, mut oks) = (0u64, 0u64);
        loop {
            while !*active.borrow() {
                if active.changed().await.is_err() {
                    return;
                }
            }
            let mut det = StallDetector::default();
            let mut healthy_since = Instant::now();
            let mut reported_ok = false;
            let (mut slow_max, mut last_stat) = (Duration::ZERO, Instant::now());
            while *active.borrow() {
                if conn.is_none() {
                    conn = zbus::Connection::session().await.ok();
                }
                let sample = match &conn {
                    Some(c) => probe_main_loop(c).await,
                    None => None,
                };
                if let Some(d) = sample {
                    slow_max = slow_max.max(d);
                }
                if det.feed(sample) {
                    warn!("guard: compositor did not answer {} probes in a row - tripping the breaker", STALLS_TO_TRIP);
                    trips += 1;
                    let _ = trip_tx.send(trips);
                    // Wait for the owner to take the display down.
                    while *active.borrow() {
                        if active.changed().await.is_err() {
                            return;
                        }
                    }
                    break;
                }
                if det.consecutive() == 0 {
                    if healthy_since.elapsed() >= HEALTHY_AFTER && !reported_ok {
                        reported_ok = true;
                        oks += 1;
                        let _ = ok_tx.send(oks);
                    }
                } else {
                    healthy_since = Instant::now();
                    reported_ok = false;
                    warn!(
                        "guard: compositor probe stalled ({} in a row)",
                        det.consecutive()
                    );
                }
                if last_stat.elapsed() >= Duration::from_secs(60) {
                    info!("guard: compositor main-loop probe, worst round trip in the last minute: {} ms", slow_max.as_millis());
                    slow_max = Duration::ZERO;
                    last_stat = Instant::now();
                }
                tokio::select! {
                    _ = tokio::time::sleep(WATCHDOG_PERIOD) => {}
                    _ = active.changed() => {}
                }
            }
        }
    });
    (trip_rx, ok_rx)
}

/// Tests for the boot verdict, the hotplug limiter, the stall detector and the breaker, using a private fake session bus where D-Bus is needed.
#[cfg(test)]
mod tests;
