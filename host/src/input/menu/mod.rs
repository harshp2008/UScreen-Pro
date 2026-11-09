//! The pen's long-press menu: hold the pen still on the screen for about 0.6 s, lift, and a small
//! menu (cut / copy / paste / select all / undo) appears at the cursor.
//!
//! How the pieces fit:
//!
//! * `ghost/auto.rs` notices the long press (the tip never left the click-slop zone) and calls
//!   [`request`] when the pen lifts.
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
/// non-blocking; does nothing when no runner is listening.
pub(in crate::input) fn request(tx: &Option<std::sync::mpsc::Sender<()>>) {
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
}
