//! Turning decoded [`InputEvent`]s into evdev writes on the virtual devices (touch slots, pen
//! tilt/pressure/eraser, pointer buttons, keys). `handle_event` is only the dispatcher; each kind of
//! message has its own handler:
//!
//! * `pen` - pen samples, their routing by input style, and the pen-action counters.
//! * `touch` - touchscreen contacts.
//! * `touchpad` - touchpad mode: relative motion, buttons, scroll, raw fingers, gestures, wake.
//! * `link` - everything about the link and settings: resolution, streaming config, mode, pings.

use super::*;

mod link;
mod pen;
#[cfg(test)]
mod tests;
mod touch;
mod touchpad;

pub(in crate::input) use pen::apply_pen;

/// Handle one decoded message from the tablet: bookkeeping common to all input, then the
/// handler of its kind.
pub(super) fn handle_event(
    instance: u32,
    event: InputEvent,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
    settings_tx: &Option<watch::Sender<EncoderSettings>>,
    latency: &crate::capture::latency::LatencyTracker,
    ping_notify: &Arc<tokio::sync::Notify>,
) {
    // A pinch whose end never arrived must not leave Ctrl held.
    if !matches!(event, InputEvent::Gesture { .. }) {
        if let Ok(mut guard) = uinput.lock() {
            guard.expire_pinch();
        }
    }
    // Any input from the tablet counts as use for the idle-dim timer.
    if matches!(
        event,
        InputEvent::Touch { .. }
            | InputEvent::Pen { .. }
            | InputEvent::Rel { .. }
            | InputEvent::Button { .. }
            | InputEvent::Scroll { .. }
            | InputEvent::TouchpadRaw { .. }
    ) {
        crate::config::options::note_activity(instance);
    }
    match event {
        InputEvent::Touch {
            x,
            y,
            pressure,
            action,
            slot,
        } => touch::handle_touch(instance, x, y, pressure, action, slot, uinput),
        InputEvent::Pen {
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
            eraser,
            action,
        } => pen::handle_pen(
            instance, x, y, pressure, tilt_x, tilt_y, eraser, action, uinput,
        ),
        InputEvent::Resolution {
            width,
            height,
            width_mm,
            height_mm,
            preferred_transport,
            version,
        } => link::handle_resolution(
            width,
            height,
            width_mm,
            height_mm,
            preferred_transport,
            version,
            uinput,
            settings_tx,
        ),
        InputEvent::Rendered { seq, decode_us } => latency.on_rendered(seq, decode_us),
        InputEvent::Config {
            bitrate,
            fps,
            encoder,
        } => link::handle_config(bitrate, fps, encoder, settings_tx),
        // Already consumed by handle_connection; a second one is harmless.
        InputEvent::Auth { .. } => {}
        InputEvent::TouchpadRaw { slots } => touchpad::handle_touchpad_raw(instance, slots, uinput),
        InputEvent::Rel { dx, dy } => touchpad::handle_rel(dx, dy, uinput),
        InputEvent::Button { button, down } => touchpad::handle_button(button, down, uinput),
        InputEvent::Scroll { dx, dy } => touchpad::handle_scroll(dx, dy, uinput),
        InputEvent::Mode { pen_only } => link::handle_mode(instance, pen_only),
        InputEvent::Gesture { kind, amount } => touchpad::handle_gesture(&kind, amount, uinput),
        InputEvent::Wake => touchpad::handle_wake(instance, uinput),
        InputEvent::Setting { key, value } => {
            crate::config::options::set_from_device(instance, &key, &value);
        }
        InputEvent::Ping => link::handle_ping(ping_notify),
        InputEvent::PreferredTransport { transport } => {
            link::handle_preferred_transport(&transport)
        }
    }
}
