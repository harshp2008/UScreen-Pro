//! The pen's long-press menu: hold the pen still on the screen for about 0.8 s and a small menu
//! (cut / copy / paste / select all / undo) appears at the cursor, while the pen is still down.
//!
//! How the pieces fit:
//!
//! * `ghost/auto.rs` holds back the left-button press of a mouse-route touch. When the tip has stayed
//!   inside the click-slop zone for the long-press time it calls [`request`]; a click is only sent
//!   once it is known not to be a long press, so the text under the pen is never clicked.
//! * `runner` is the background thread that answers the request: asks the desktop backend what is
//!   under the pointer, lets `decision` choose the buttons, shows the `uscreen-menu` window at the
//!   pointer, waits for the choice, gives focus back to the app and sends the key combination.
//! * `decision` and `actions` are pure (no desktop, no I/O), so they are unit-tested.
//!
//! The menu needs a desktop backend that can describe the pointer's surroundings and place a
//! window ([`crate::desktop::Capabilities`]); without one it simply never appears.

mod actions;
mod decision;
mod runner;

pub(in crate::input) use runner::spawn;

/// Ask the runner of this tablet to show the menu for the pen's current position. Cheap and
/// non-blocking. False when nobody is listening (the caller then carries on as if the menu had
/// been declined).
pub(in crate::input) fn request(tx: &Option<std::sync::mpsc::Sender<()>>) -> bool {
    tx.as_ref().is_some_and(|tx| tx.send(()).is_ok())
}
