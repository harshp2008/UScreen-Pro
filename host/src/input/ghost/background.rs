//! The two background loops of pen routing: a thread that follows the focused app (through the
//! desktop backend) and the request files, and a 50 ms task that carries out timed pen work (the
//! range-exit grace, the route check for a still pen, the idle cursor hide).

use super::control::{dump_file, dump_request_file, toggle_file};
use super::route::{route_to_pen, set_focus, toggle_route};
use super::*;
use crate::desktop::FocusUpdate;
use std::sync::OnceLock;
use std::time::Duration;

/// Start the background watcher once per process: it polls the desktop backend for the focused
/// app and watches the route-toggle and recorder-dump request files.
pub(in crate::input) fn start_focus_watcher() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("uscreen-focus".into())
        .spawn(|| {
            let mut desktop = crate::desktop::detect();
            info!(
                "Desktop backend: {} (focus reporting: {})",
                desktop.name(),
                desktop.capabilities().focus
            );
            let mut last_toggle = std::fs::read_to_string(toggle_file().unwrap_or_default()).ok();
            let mut last_dump =
                std::fs::read_to_string(dump_request_file().unwrap_or_default()).ok();
            loop {
                std::thread::sleep(Duration::from_millis(150));
                match desktop.poll_focus() {
                    FocusUpdate::Unchanged => {}
                    FocusUpdate::Known(app) => set_focus(Some(app)),
                    FocusUpdate::Unknown => set_focus(None),
                }
                if let (Some(req), Some(out)) = (dump_request_file(), dump_file()) {
                    let now = std::fs::read_to_string(&req).ok();
                    if now != last_dump {
                        last_dump = now;
                        if last_dump.is_some() {
                            let text = crate::input::recorder::render();
                            match std::fs::write(&out, &text) {
                                Ok(()) => info!(
                                    "Pen recorder written to {:?} ({} lines)",
                                    out,
                                    text.lines().count()
                                ),
                                Err(e) => {
                                    warn!("Could not write the pen recorder to {:?}: {}", out, e)
                                }
                            }
                        }
                    }
                }
                if let Some(path) = toggle_file() {
                    let now = std::fs::read_to_string(&path).ok();
                    if now != last_toggle {
                        last_toggle = now;
                        if last_toggle.is_some() {
                            let art = crate::config::FileConfig::load().stylus_art_apps;
                            let tablet = toggle_route(&art);
                            info!(
                                "Pen route for the focused app is now {}",
                                if tablet { "tablet (pressure)" } else { "mouse" }
                            );
                        }
                    }
                }
            }
        });
    if let Err(e) = spawned {
        warn!("Could not start the focus watcher: {}", e);
    }
}

/// Drive the idle hide for the daemon's lifetime: a few ticks a second, reading the timeout live.
pub(in crate::input) fn spawn_idle_task(
    uinput: Arc<std::sync::Mutex<InjectDevices>>,
    instance: u32,
) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        let mut n: u32 = 0;
        loop {
            tick.tick().await;
            n = n.wrapping_add(1);
            // The idle hide is checked a few times a second, reading the timeout live (before the
            // device lock is taken, as the event path does).
            let opts = crate::config::options::current_options(instance);
            let timeout = if n.is_multiple_of(5) {
                opts.stylus_idle_hide_timeout
            } else {
                0
            };
            let to_pen = route_to_pen(&opts.stylus_art_apps);
            let Ok(mut guard) = uinput.lock() else {
                continue;
            };
            guard.auto_check_route(to_pen);
            guard.auto_flush_exit();
            if timeout != 0 && guard.ghost_idle_tick(timeout) {
                info!("Pen out of range for {}s — cursor hidden", timeout);
            }
        }
    });
}
