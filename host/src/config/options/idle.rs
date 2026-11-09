//! The idle-dim state machine that fades the tablet's screen when it is not being used

use super::*;

/// Start the idle-dim task of a slot (once). It dims the tablet after the pointer has been away
/// and the tablet unused for the configured time, and undims it when either comes back.
pub fn spawn_idle_task(instance: u32) {
    let first = with_slot(instance, |s| !std::mem::replace(&mut s.idle_task, true));
    if !first {
        return;
    }
    let opts = options_rx(instance);
    let (pointer, activity) = with_slot(instance, |s| (s.pointer.subscribe(), s.activity.clone()));
    tokio::spawn(idle_loop(opts, pointer, activity, move |msg| {
        crate::net::notices::send(instance, msg)
    }));
}

pub(super) async fn idle_loop(
    mut opts_rx: watch::Receiver<DeviceOptions>,
    mut pointer_rx: watch::Receiver<bool>,
    activity: Arc<Notify>,
    send: impl Fn(Value),
) {
    let mut dimmed = false;
    loop {
        let o = opts_rx.borrow().clone();
        // Without Display mode there is no virtual screen for the pointer to be on, so it is always
        // "away" and only the tablet's own input keeps the timer from running out.
        let pointer_away = !*pointer_rx.borrow() || o.input_mode != InputMode::Display;
        // Touchpad mode has no screen for the pointer to be on; there the timer is simply "no
        // input from the tablet for the timeout".
        let armed = o.idle_dim_enabled && pointer_away;
        if !armed {
            if dimmed {
                dimmed = false;
                send(o.undim_message());
            }
            tokio::select! {
                r = opts_rx.changed() => if r.is_err() { return },
                r = pointer_rx.changed() => if r.is_err() { return },
            }
            continue;
        }
        if dimmed {
            tokio::select! {
                _ = activity.notified() => {
                    dimmed = false;
                    send(o.undim_message());
                }
                r = opts_rx.changed() => if r.is_err() { return },
                r = pointer_rx.changed() => if r.is_err() { return },
            }
            continue;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(o.idle_dim_timeout_secs as u64)) => {
                dimmed = true;
                send(o.dim_message());
            }
            _ = activity.notified() => {}
            r = opts_rx.changed() => if r.is_err() { return },
            r = pointer_rx.changed() => if r.is_err() { return },
        }
    }
}
