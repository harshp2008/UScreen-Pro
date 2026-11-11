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

/// The idle-dim loop of one tablet: dims after the idle timeout, undims on activity, and follows
/// option changes.
pub(super) async fn idle_loop(
    mut opts_rx: watch::Receiver<DeviceOptions>,
    mut pointer_rx: watch::Receiver<bool>,
    activity: Arc<Notify>,
    send: impl Fn(Value),
) {
    let mut dimmed = false;
    loop {
        let o = opts_rx.borrow().clone();
        // Idle dimming belongs to Display mode: only there is the tablet a screen the pointer can
        // leave. Touchpad and Stylus mode never dim (the tablet is the input device, not a display).
        let armed =
            o.idle_dim_enabled && o.input_mode == InputMode::Display && !*pointer_rx.borrow();
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
