//! What a pen side button can do besides clicking: send a key combination (undo, redo, copy, paste, cut, select all) or open the
//! pen menu. The button names come from `stylus_button_primary` / `stylus_button_secondary`; `right`, `middle` and `none` stay
//! mouse buttons and are handled where the pen routes its clicks.

use super::*;

/// A non-mouse action of a pen side button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::input) enum ButtonAction {
    /// Tap these keys together (modifiers first).
    Keys(&'static [u16]),
    /// Open the pen menu at the pen.
    Menu,
}

/// The action for a button name, or `None` for the mouse buttons and anything unknown.
pub(in crate::input) fn parse(name: &str) -> Option<ButtonAction> {
    Some(match name {
        "undo" => ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_Z]),
        "redo" => ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_Z]),
        "copy" => ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_C]),
        "paste" => ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_V]),
        "cut" => ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_X]),
        "select-all" => ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_A]),
        "menu" => ButtonAction::Menu,
        _ => return None,
    })
}

impl InjectDevices {
    /// Carry out the action named `name` when its button goes `down` (the release does nothing). Returns whether `name` is such an
    /// action at all, so the caller leaves the mouse buttons to their own code.
    pub(in crate::input) fn run_button_action(&mut self, name: &str, down: bool) -> bool {
        let Some(action) = parse(name) else {
            return false;
        };
        if !down {
            return true;
        }
        match action {
            ButtonAction::Keys(keys) => self.tap_key_combo(keys),
            ButtonAction::Menu => {
                if !crate::input::menu::request(&self.menu_tx) {
                    tracing::debug!("Pen button menu request ignored: the menu is not available");
                }
            }
        }
        true
    }
}

/// Tests for the button action names.
#[cfg(test)]
mod tests {
    use super::*;

    /// Each named action maps to its keys, with the modifiers first.
    #[test]
    fn names_map_to_key_combinations() {
        assert_eq!(
            parse("undo"),
            Some(ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_Z]))
        );
        assert_eq!(
            parse("redo"),
            Some(ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_Z]))
        );
        assert_eq!(
            parse("copy"),
            Some(ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_C]))
        );
        assert_eq!(
            parse("select-all"),
            Some(ButtonAction::Keys(&[KEY_LEFTCTRL, KEY_A]))
        );
        assert_eq!(parse("menu"), Some(ButtonAction::Menu));
    }

    /// The mouse buttons and unknown names are not actions here.
    #[test]
    fn mouse_buttons_and_unknown_names_are_not_actions() {
        for n in ["right", "middle", "none", "", "reboot"] {
            assert_eq!(parse(n), None, "{n}");
        }
    }
}
