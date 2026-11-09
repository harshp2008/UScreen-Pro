//! The actions the pen menu can offer, their names on the wire (the menu window reports the chosen
//! one by name) and the key combination each one sends to the focused app.

use crate::input::uinput::{KEY_A, KEY_C, KEY_LEFTCTRL, KEY_V, KEY_X, KEY_Z};

/// One button of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::input) enum MenuAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
}

impl MenuAction {
    /// The order the buttons are shown in.
    pub(in crate::input) const ORDER: [MenuAction; 5] = [
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::SelectAll,
        Self::Undo,
    ];

    /// The action's name in command-line arguments and in the menu's result.
    pub(in crate::input) fn name(self) -> &'static str {
        match self {
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::SelectAll => "select-all",
            Self::Undo => "undo",
        }
    }

    /// The action with this name, if there is one.
    pub(in crate::input) fn from_name(name: &str) -> Option<Self> {
        Self::ORDER.into_iter().find(|a| a.name() == name)
    }

    /// The keys to tap: Ctrl plus a letter.
    pub(in crate::input) fn keys(self) -> [u16; 2] {
        let letter = match self {
            Self::Cut => KEY_X,
            Self::Copy => KEY_C,
            Self::Paste => KEY_V,
            Self::SelectAll => KEY_A,
            Self::Undo => KEY_Z,
        };
        [KEY_LEFTCTRL, letter]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for a in MenuAction::ORDER {
            assert_eq!(MenuAction::from_name(a.name()), Some(a));
        }
        assert_eq!(MenuAction::from_name("reboot"), None);
    }

    #[test]
    fn every_action_is_ctrl_plus_its_letter() {
        assert_eq!(MenuAction::Copy.keys(), [KEY_LEFTCTRL, KEY_C]);
        assert_eq!(MenuAction::Undo.keys(), [KEY_LEFTCTRL, KEY_Z]);
        assert_eq!(MenuAction::SelectAll.keys()[1], KEY_A);
    }
}
