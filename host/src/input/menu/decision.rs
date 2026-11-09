//! Whether to show the pen menu and which buttons to offer, from what the desktop says is under the
//! pointer. Pure logic, no I/O.
//!
//! Rules (Tier 1: surface kind, window geometry, selection and clipboard sizes):
//!
//! * Only over an application window, and not in the top strip of it (a title bar or header).
//! * Text highlighted: Cut, Copy, Select All (and Paste if the clipboard has something).
//! * Nothing highlighted but the clipboard has something: Paste, Select All.
//! * Nothing to act on: no menu at all, so a long press behaves like a plain click.
//! * Whenever the menu shows, Undo is offered too.

use super::actions::MenuAction;
use crate::desktop::{PointerContext, Zone};

/// Tunables of the decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::input) struct Rules {
    /// The top strip of a window, in pixels, treated as a title bar / header: no menu there.
    pub titlebar_px: f64,
}

impl Default for Rules {
    fn default() -> Self {
        Self { titlebar_px: 48.0 }
    }
}

/// The buttons to show, in display order; empty means "do not show a menu".
pub(in crate::input) fn items(ctx: &PointerContext, rules: &Rules) -> Vec<MenuAction> {
    if ctx.zone != Zone::Window {
        return Vec::new();
    }
    if ctx.depth_in_window().is_some_and(|d| d < rules.titlebar_px) {
        return Vec::new();
    }
    let selected = ctx.selection_len > 0;
    let pasteable = ctx.clipboard_len > 0;
    if !selected && !pasteable {
        return Vec::new();
    }
    MenuAction::ORDER
        .into_iter()
        .filter(|a| match a {
            MenuAction::Cut | MenuAction::Copy => selected,
            MenuAction::Paste => pasteable,
            MenuAction::SelectAll | MenuAction::Undo => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::PointerContext;
    use MenuAction::*;

    fn ctx(zone: Zone, selection: usize, clipboard: usize, depth: Option<f64>) -> PointerContext {
        let mut c = PointerContext {
            zone,
            selection_len: selection,
            clipboard_len: clipboard,
            y: 500.0,
            ..Default::default()
        };
        if let Some(d) = depth {
            c.frame = Some(crate::desktop::Rect {
                x: 0.0,
                y: 500.0 - d,
                w: 800.0,
                h: 600.0,
            });
        }
        c
    }

    #[test]
    fn highlighted_text_offers_cut_copy_select_all_undo() {
        let r = items(&ctx(Zone::Window, 5, 0, Some(300.0)), &Rules::default());
        assert_eq!(r, vec![Cut, Copy, SelectAll, Undo]);
    }

    #[test]
    fn a_full_clipboard_adds_paste() {
        let r = items(&ctx(Zone::Window, 5, 9, Some(300.0)), &Rules::default());
        assert_eq!(r, vec![Cut, Copy, Paste, SelectAll, Undo]);
    }

    #[test]
    fn only_a_clipboard_offers_paste_select_all_undo() {
        let r = items(&ctx(Zone::Window, 0, 9, Some(300.0)), &Rules::default());
        assert_eq!(r, vec![Paste, SelectAll, Undo]);
    }

    #[test]
    fn nothing_to_act_on_means_no_menu() {
        assert!(items(&ctx(Zone::Window, 0, 0, Some(300.0)), &Rules::default()).is_empty());
    }

    #[test]
    fn never_outside_an_app_window() {
        for z in [Zone::Shell, Zone::Desktop, Zone::Overview] {
            assert!(
                items(&ctx(z, 5, 5, None), &Rules::default()).is_empty(),
                "{z:?}"
            );
        }
    }

    #[test]
    fn never_in_the_title_strip_of_a_window() {
        assert!(items(&ctx(Zone::Window, 5, 5, Some(20.0)), &Rules::default()).is_empty());
        assert!(!items(&ctx(Zone::Window, 5, 5, Some(60.0)), &Rules::default()).is_empty());
    }
}
