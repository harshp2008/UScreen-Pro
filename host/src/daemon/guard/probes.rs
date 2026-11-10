//! Compositor probes over D-Bus and the wait until the compositor is ready

use super::*;

/// Whether `XDG_CURRENT_DESKTOP` says GNOME.
pub(super) fn is_gnome_wayland() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase()
        .contains("GNOME")
        && std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

/// Round-trip of a call that GNOME Shell serves from its main loop.
pub(super) async fn probe_main_loop(conn: &zbus::Connection) -> Sample {
    let t0 = Instant::now();
    let call = conn.call_method(
        Some("org.gnome.Mutter.DisplayConfig"),
        "/org/gnome/Mutter/DisplayConfig",
        Some("org.gnome.Mutter.DisplayConfig"),
        "GetCurrentState",
        &(),
    );
    match tokio::time::timeout(PROBE_TIMEOUT, call).await {
        Ok(Ok(_)) => Some(t0.elapsed()),
        _ => None,
    }
}

/// GNOME Shell's uptime according to the session bus, `None` if it is not running.
pub(super) async fn shell_uptime(conn: &zbus::Connection) -> Option<Duration> {
    let dbus = zbus::fdo::DBusProxy::new(conn).await.ok()?;
    let name: zbus::names::BusName = "org.gnome.Shell".try_into().ok()?;
    if !dbus.name_has_owner(name.clone()).await.ok()? {
        return None;
    }
    let pid = dbus.get_connection_unix_process_id(name).await.ok()?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 22 (starttime) counts from after the ")" that closes the comm field.
    let after = stat.rsplit_once(')')?.1;
    let start_ticks: u64 = after.split_whitespace().nth(19)?.parse().ok()?;
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let up: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(Duration::from_secs_f64(
        (up - start_ticks as f64 / hz).max(0.0),
    ))
}

/// Whether the GNOME session manager reports the session as running.
pub(super) async fn session_running(conn: &zbus::Connection) -> Option<bool> {
    // A method, not a property, on org.gnome.SessionManager (verified with
    // busctl introspect on GNOME 46).
    let reply = conn
        .call_method(
            Some("org.gnome.SessionManager"),
            "/org/gnome/SessionManager",
            Some("org.gnome.SessionManager"),
            "IsSessionRunning",
            &(),
        )
        .await
        .ok()?;
    reply.body().deserialize::<bool>().ok()
}

/// Block until it is safe to touch the display topology. Returns false if a
/// shutdown was requested first. Never "fails": the answer to an unready
/// compositor is to wait.
pub async fn wait_compositor_ready(shutdown: &mut watch::Receiver<bool>) -> bool {
    let started = Instant::now();
    let mut last_log = Instant::now() - Duration::from_secs(3600);
    let mut ok_streak = 0u32;
    let mut conn: Option<zbus::Connection> = None;
    let gnome = is_gnome_wayland();

    loop {
        if *shutdown.borrow() {
            return false;
        }
        let verdict: Result<(), String> = if gnome {
            if conn.is_none() {
                conn = zbus::Connection::session().await.ok();
            }
            match &conn {
                None => Err("no session bus yet".into()),
                Some(c) => match shell_uptime(c).await {
                    None => Err("gnome-shell is not on the bus yet".into()),
                    Some(_) => match session_running(c).await {
                        Some(false) => Err("gnome-session is still starting".into()),
                        _ => match probe_main_loop(c).await {
                            Some(d) if d <= READY_PROBE_MAX => Ok(()),
                            Some(d) => Err(format!("shell main loop busy ({} ms)", d.as_millis())),
                            None => Err("shell main loop not answering".into()),
                        },
                    },
                },
            }
        } else {
            // Other desktops: a compositor socket to talk to is the best generic signal.
            let wl = std::env::var_os("WAYLAND_DISPLAY")
                .zip(std::env::var_os("XDG_RUNTIME_DIR"))
                .map(|(d, r)| PathBuf::from(r).join(d).exists());
            match wl {
                Some(false) => Err("wayland socket not present yet".into()),
                _ if started.elapsed() < Duration::from_secs(5) => Err("settling".into()),
                _ => Ok(()),
            }
        };

        match verdict {
            Ok(()) => {
                ok_streak += 1;
                if ok_streak >= if gnome { READY_PROBES_NEEDED } else { 1 } {
                    info!(
                        "guard: compositor ready after {:.1}s - safe to create the virtual display",
                        started.elapsed().as_secs_f32()
                    );
                    return true;
                }
            }
            Err(why) => {
                ok_streak = 0;
                if last_log.elapsed() > Duration::from_secs(15) {
                    last_log = Instant::now();
                    info!(
                        "guard: waiting for the compositor ({}), {}s so far",
                        why,
                        started.elapsed().as_secs()
                    );
                }
                // A shell that answers, just never within the idle threshold, must
                // not hold the display hostage forever.
                if gnome && started.elapsed() > READY_SLOW_FALLBACK && why.contains("busy") {
                    warn!(
                        "guard: proceeding on a busy-but-answering shell after {}s",
                        started.elapsed().as_secs()
                    );
                    return true;
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(READY_PROBE_GAP) => {}
            _ = shutdown.changed() => {}
        }
    }
}
