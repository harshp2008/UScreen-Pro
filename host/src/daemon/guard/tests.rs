//! Unit tests of `guard/mod.rs`. Moved out of the file so the code and its tests read separately; they see its private items through `use super::*`.

use super::*;

/// Tests below mutate process-wide environment (bus address, desktop name).
static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Builds a guard record in `state` stamped with `boot`; zero timestamp and strikes.
fn rec(state: RunState, boot: &str) -> GuardRecord {
    GuardRecord {
        boot_id: boot.into(),
        state,
        since_unix: 0,
        strikes: 0,
    }
}

/// With no record on disk the verdict is `Normal`.
#[test]
fn no_history_is_normal() {
    assert_eq!(evaluate(None, "b2"), Verdict::Normal);
}

/// A display still marked up when a different boot starts means the last session ended in a crash or hard reset, so the next start is safe mode.
#[test]
fn display_up_on_a_previous_boot_means_safe_mode() {
    let r = rec(RunState::DisplayUp, "b1");
    assert!(matches!(evaluate(Some(&r), "b2"), Verdict::Safe(_)));
}

/// The same boot id means only the daemon restarted, which is not a reason for safe mode.
#[test]
fn display_up_in_the_same_boot_is_just_a_daemon_restart() {
    let r = rec(RunState::DisplayUp, "b1");
    assert_eq!(evaluate(Some(&r), "b1"), Verdict::Normal);
}

/// Runs that ended healthy or were stopped cleanly never cause safe mode.
#[test]
fn healthy_and_stopped_runs_never_trigger_safe_mode() {
    for s in [RunState::Healthy, RunState::Stopped] {
        assert_eq!(evaluate(Some(&rec(s, "b1")), "b2"), Verdict::Normal);
    }
}

/// A tripped breaker forces safe mode on the next start, even within the same boot.
#[test]
fn a_tripped_run_is_safe_even_within_the_same_boot() {
    let r = rec(RunState::Tripped, "b1");
    assert!(matches!(evaluate(Some(&r), "b1"), Verdict::Safe(_)));
    assert!(matches!(evaluate(Some(&r), "b2"), Verdict::Safe(_)));
}

/// The record survives a JSON round trip and the state is written as `display_up` on disk.
#[test]
fn record_round_trips_through_json() {
    let r = rec(RunState::DisplayUp, "abc");
    let back: GuardRecord = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(r, back);
    assert!(serde_json::to_string(&r).unwrap().contains("display_up"));
}

/// Hotplugs are spaced by the minimum gap; once the per-window cap is reached the delay runs to the cool-down measured from the last change, and history is forgotten when the window has passed.
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

/// Stall samples must be consecutive to trip the breaker; a fast or missing sample resets the count.
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
    /// Kills and reaps the fake session bus.
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Fake `org.gnome.Mutter.DisplayConfig` service whose `GetCurrentState` answers after a fixed delay, to exercise the probe timeouts.
struct FakeConfig(Duration);
#[zbus::interface(name = "org.gnome.Mutter.DisplayConfig")]
impl FakeConfig {
    /// Answers after the configured delay with an empty result.
    async fn get_current_state(&self) -> u32 {
        tokio::time::sleep(self.0).await;
        0
    }
}
/// Starts a private session `dbus-daemon` with a fake display-config service answering after `delay`; returns `None` when `dbus-daemon` is not installed so the test is skipped.
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

/// The first two trips return the 60 s and 300 s back-off from `TRIP_BACKOFF`; the third returns `None` and latches the display off.
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
