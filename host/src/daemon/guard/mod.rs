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
const READY_PROBE_GAP: Duration = Duration::from_millis(250);
/// Never proceed on a sluggish-but-answering shell before this long.
const READY_SLOW_FALLBACK: Duration = Duration::from_secs(180);

/// Minimum spacing between two virtual-monitor topology changes.
const HOTPLUG_MIN_GAP: Duration = Duration::from_secs(3);
/// More than this many changes inside `HOTPLUG_WINDOW` triggers a cool-down.
const HOTPLUG_MAX_PER_WINDOW: usize = 4;
const HOTPLUG_WINDOW: Duration = Duration::from_secs(60);
const HOTPLUG_COOLDOWN: Duration = Duration::from_secs(30);

const WATCHDOG_PERIOD: Duration = Duration::from_secs(1);
/// A probe that takes longer than this (or times out) is a stall sample.
const STALL_PROBE: Duration = Duration::from_millis(2000);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests below mutate process-wide environment (bus address, desktop name).
    static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn rec(state: RunState, boot: &str) -> GuardRecord {
        GuardRecord {
            boot_id: boot.into(),
            state,
            since_unix: 0,
            strikes: 0,
        }
    }

    #[test]
    fn no_history_is_normal() {
        assert_eq!(evaluate(None, "b2"), Verdict::Normal);
    }

    #[test]
    fn display_up_on_a_previous_boot_means_safe_mode() {
        let r = rec(RunState::DisplayUp, "b1");
        assert!(matches!(evaluate(Some(&r), "b2"), Verdict::Safe(_)));
    }

    #[test]
    fn display_up_in_the_same_boot_is_just_a_daemon_restart() {
        let r = rec(RunState::DisplayUp, "b1");
        assert_eq!(evaluate(Some(&r), "b1"), Verdict::Normal);
    }

    #[test]
    fn healthy_and_stopped_runs_never_trigger_safe_mode() {
        for s in [RunState::Healthy, RunState::Stopped] {
            assert_eq!(evaluate(Some(&rec(s, "b1")), "b2"), Verdict::Normal);
        }
    }

    #[test]
    fn a_tripped_run_is_safe_even_within_the_same_boot() {
        let r = rec(RunState::Tripped, "b1");
        assert!(matches!(evaluate(Some(&r), "b1"), Verdict::Safe(_)));
        assert!(matches!(evaluate(Some(&r), "b2"), Verdict::Safe(_)));
    }

    #[test]
    fn record_round_trips_through_json() {
        let r = rec(RunState::DisplayUp, "abc");
        let back: GuardRecord = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(r, back);
        assert!(serde_json::to_string(&r).unwrap().contains("display_up"));
    }

    #[test]
    fn hotplugs_are_spaced_and_capped() {
        let mut l = HotplugLimiter::default();
        let t0 = Instant::now();
        assert!(l.delay(t0).is_zero());
        l.record(t0);
        // 1 s later the minimum gap still has 2 s to run.
        assert_eq!(l.delay(t0 + Duration::from_secs(1)), Duration::from_secs(2));
        assert!(l.delay(t0 + Duration::from_secs(4)).is_zero());
        for i in 1..HOTPLUG_MAX_PER_WINDOW {
            l.record(t0 + Duration::from_secs(4 * i as u64));
        }
        // Cap reached: wait out the cool-down measured from the last change.
        let last = t0 + Duration::from_secs(4 * (HOTPLUG_MAX_PER_WINDOW as u64 - 1));
        assert_eq!(
            l.delay(last + Duration::from_secs(5)),
            Duration::from_secs(25)
        );
        // Once the window has passed, history is forgotten.
        assert!(l
            .delay(last + HOTPLUG_WINDOW + Duration::from_secs(1))
            .is_zero());
    }

    #[test]
    fn breaker_trips_only_on_consecutive_stalls() {
        let mut d = StallDetector::default();
        let slow = Some(Duration::from_millis(2500));
        let fast = Some(Duration::from_millis(20));
        assert!(!d.feed(slow));
        assert!(!d.feed(None));
        assert!(!d.feed(fast), "a good sample resets the count");
        assert!(!d.feed(slow));
        assert!(!d.feed(None));
        assert!(d.feed(slow), "third in a row trips");
        assert!(!d.feed(slow), "and trips once, not on every later sample");
    }

    /// The core promise: with no compositor on the bus the gate keeps waiting
    /// (and announces it), it does not give up and go ahead.
    #[tokio::test]
    async fn gate_waits_while_the_shell_is_absent() {
        let _env = ENV.lock().await;
        std::env::set_var("XDG_CURRENT_DESKTOP", "ubuntu:GNOME");
        std::env::set_var("XDG_SESSION_TYPE", "wayland");
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/uscreen-test-bus",
        );
        let (_tx, mut rx) = watch::channel(false);
        let r = tokio::time::timeout(Duration::from_secs(3), wait_compositor_ready(&mut rx)).await;
        assert!(
            r.is_err(),
            "gate must still be waiting after 3s, got {:?}",
            r
        );
    }

    /// And a shutdown request ends the wait instead of hanging the daemon.
    #[tokio::test]
    async fn gate_wait_ends_on_shutdown() {
        let _env = ENV.lock().await;
        std::env::set_var("XDG_CURRENT_DESKTOP", "ubuntu:GNOME");
        std::env::set_var("XDG_SESSION_TYPE", "wayland");
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/uscreen-test-bus",
        );
        let (tx, mut rx) = watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            let _ = tx.send(true);
        });
        let r = tokio::time::timeout(Duration::from_secs(5), wait_compositor_ready(&mut rx)).await;
        assert_eq!(r, Ok(false));
    }

    /// A private bus with a fake DisplayConfig whose answer is `delay` late.
    struct FakeShell(std::process::Child);
    impl Drop for FakeShell {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct FakeConfig(Duration);
    #[zbus::interface(name = "org.gnome.Mutter.DisplayConfig")]
    impl FakeConfig {
        async fn get_current_state(&self) -> u32 {
            tokio::time::sleep(self.0).await;
            0
        }
    }
    async fn fake_bus(delay: Duration) -> Option<(FakeShell, zbus::Connection, String)> {
        use std::io::BufRead;
        let mut child = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .ok()?;
        let mut addr = String::new();
        std::io::BufReader::new(child.stdout.take()?)
            .read_line(&mut addr)
            .ok()?;
        let addr = addr.trim().to_string();
        let guard = FakeShell(child);
        let srv = zbus::connection::Builder::address(addr.as_str())
            .ok()?
            .name("org.gnome.Mutter.DisplayConfig")
            .ok()?
            .serve_at("/org/gnome/Mutter/DisplayConfig", FakeConfig(delay))
            .ok()?
            .build()
            .await
            .ok()?;
        Some((guard, srv, addr))
    }

    /// End-to-end on a private bus: a main loop that answers promptly never
    /// trips the breaker; one that stops answering trips it after three stalls.
    #[tokio::test]
    async fn watchdog_trips_on_a_frozen_compositor_and_not_on_a_healthy_one() {
        let _env = ENV.lock().await;
        std::env::set_var("XDG_CURRENT_DESKTOP", "ubuntu:GNOME");
        std::env::set_var("XDG_SESSION_TYPE", "wayland");

        // Healthy: 5 ms answers, watched for 6 s => no trip.
        let Some((_bus, _srv, addr)) = fake_bus(Duration::from_millis(5)).await else {
            eprintln!("dbus-daemon unavailable; skipping");
            return;
        };
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &addr);
        let (active_tx, active_rx) = watch::channel(true);
        let (mut trip_rx, _ok) = spawn_watchdog(active_rx);
        let r = tokio::time::timeout(Duration::from_secs(6), trip_rx.changed()).await;
        assert!(r.is_err(), "healthy compositor must not trip the breaker");
        let _ = active_tx.send(false);
        drop((_bus, _srv));

        // Frozen: answers 60 s late => 3 probes x 3 s timeout (+1 s period).
        let Some((_bus, _srv, addr)) = fake_bus(Duration::from_secs(60)).await else {
            return;
        };
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &addr);
        let (_active_tx, active_rx) = watch::channel(true);
        let (mut trip_rx, _ok) = spawn_watchdog(active_rx);
        let t0 = Instant::now();
        let r = tokio::time::timeout(Duration::from_secs(20), trip_rx.changed()).await;
        assert!(r.is_ok(), "frozen compositor must trip the breaker");
        let took = t0.elapsed();
        println!(
            "breaker tripped {:.1}s after the compositor froze",
            took.as_secs_f32()
        );
        assert!(
            took >= Duration::from_secs(9) && took <= Duration::from_secs(16),
            "{took:?}"
        );
    }

    /// Read-only check against the real desktop session:
    /// `cargo test guard::tests::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_probes_against_this_session() {
        let conn = zbus::Connection::session().await.unwrap();
        let up = shell_uptime(&conn).await;
        let running = session_running(&conn).await;
        let mut lat = Vec::new();
        for _ in 0..20 {
            lat.push(probe_main_loop(&conn).await);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        println!(
            "shell uptime: {:?}\nIsSessionRunning: {:?}\nprobe samples: {:?}",
            up, running, lat
        );
        assert!(up.is_some(), "gnome-shell not found on the bus");
        assert!(lat.iter().all(|s| s.is_some_and(|d| d < STALL_PROBE)));
    }

    #[test]
    fn breaker_backoff_then_latch() {
        let mut g = Guard {
            verdict: Verdict::Normal,
            limiter: HotplugLimiter::default(),
            ready_once: true,
            trips: 0,
            latched: false,
            resume_after: None,
            record: rec(RunState::DisplayUp, "b"),
        };
        // Keep the test off the real state file.
        std::env::set_var(
            "XDG_STATE_HOME",
            std::env::temp_dir().join("uscreen-guard-test"),
        );
        assert_eq!(g.tripped(), Some(Duration::from_secs(60)));
        assert_eq!(g.tripped(), Some(Duration::from_secs(300)));
        assert_eq!(g.tripped(), None);
        assert!(g.latched);
    }
}
