//! `uscreen-menu`: the small floating menu the UScreen daemon shows at the pen's cursor after a
//! long press (cut / copy / paste / select all / undo).
//!
//! It is a separate program so the daemon never links a toolkit and so a window can be placed by
//! the desktop's own tools. The daemon starts it with the buttons to show, and reads the answer
//! from a file:
//!
//! ```text
//! uscreen-menu --items cut,copy,paste --style text|icons --timeout 8 --result /path/file
//! ```
//!
//! The result file gets one line, `{"action":"copy"}` for a tapped button or `{"action":"none"}`
//! when the menu was dismissed (Escape, focus lost to another window, or the timeout).
// PLATFORM(linux): GTK 4 window; the daemon starts it and reads its answer from a file

use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Orientation};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// How long after appearing the window may lose focus without closing (it is focused a moment
/// after it is shown).
const FOCUS_GRACE: Duration = Duration::from_millis(400);

/// Look of the menu: a dark rounded pill that works in light and dark themes.
const CSS: &str = "
window.uscreen-menu { background: transparent; }
.pill {
    background: rgba(34, 34, 38, 0.97);
    border-radius: 20px;
    border: 1px solid rgba(255, 255, 255, 0.10);
    padding: 4px;
    margin: 2px;
}
.pill button {
    background: transparent;
    color: #f2f2f2;
    border: none;
    box-shadow: none;
    border-radius: 16px;
    padding: 10px 16px;
    min-height: 40px;
    font-weight: 500;
}
.pill button:hover, .pill button:active { background: rgba(255, 255, 255, 0.16); }
";

/// What the daemon asked for.
struct Args {
    items: Vec<String>,
    icons: bool,
    timeout: u64,
    result: Option<String>,
}

/// Read `--items`, `--style`, `--timeout` and `--result` from the command line.
fn parse_args() -> Args {
    let mut a = Args {
        items: Vec::new(),
        icons: false,
        timeout: 8,
        result: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().unwrap_or_default();
        match flag.as_str() {
            "--items" => {
                a.items = value
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            }
            "--style" => a.icons = value == "icons",
            "--timeout" => a.timeout = value.parse().unwrap_or(8),
            "--result" => a.result = Some(value),
            _ => {}
        }
    }
    a
}

/// The label of an action.
fn label(action: &str) -> &'static str {
    match action {
        "cut" => "Cut",
        "copy" => "Copy",
        "paste" => "Paste",
        "select-all" => "Select all",
        "undo" => "Undo",
        _ => "?",
    }
}

/// The symbolic icon of an action.
fn icon(action: &str) -> &'static str {
    match action {
        "cut" => "edit-cut-symbolic",
        "copy" => "edit-copy-symbolic",
        "paste" => "edit-paste-symbolic",
        "select-all" => "edit-select-all-symbolic",
        "undo" => "edit-undo-symbolic",
        _ => "dialog-question-symbolic",
    }
}

/// Write the one-line answer for the daemon (once), through a temporary file so it appears whole.
fn answer(result: &Option<String>, action: &str) {
    let Some(path) = result else { return };
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, format!("{{\"action\":\"{action}\"}}\n")).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Run the GTK application; the arguments come from the daemon, not from GTK.
fn main() -> glib::ExitCode {
    let args = Rc::new(parse_args());
    let app = Application::builder()
        .application_id("app.uscreen.menu")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    // The command line is ours, not GTK's.
    app.connect_activate(move |app| build_window(app, args.clone()));
    app.run_with_args::<&str>(&[])
}

/// Build the window: the pill of buttons, the dismissal rules, the timeout.
fn build_window(app: &Application, args: Rc<Args>) {
    if let Some(display) = gtk4::gdk::Display::default() {
        let css = CssProvider::new();
        #[allow(deprecated)]
        css.load_from_data(CSS);
        gtk4::style_context_add_provider_for_display(
            &display,
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let window = ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .build();
    window.add_css_class("uscreen-menu");

    // Only the first answer counts; then the window goes away.
    let answered = Rc::new(Cell::new(false));
    let finish = {
        let window = window.clone();
        let result = args.result.clone();
        let answered = answered.clone();
        Rc::new(move |action: &str| {
            if !answered.replace(true) {
                answer(&result, action);
                window.close();
            }
        })
    };

    let pill = GtkBox::new(Orientation::Horizontal, 2);
    pill.add_css_class("pill");
    for name in &args.items {
        let b = if args.icons {
            let b = Button::from_icon_name(icon(name));
            b.set_tooltip_text(Some(label(name)));
            b
        } else {
            Button::with_label(label(name))
        };
        let finish = finish.clone();
        let name = name.clone();
        b.connect_clicked(move |_| finish(&name));
        pill.append(&b);
    }
    window.set_child(Some(&pill));

    // Escape dismisses.
    let keys = gtk4::EventControllerKey::new();
    {
        let finish = finish.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk4::gdk::Key::Escape {
                finish("none");
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(keys);

    // Losing focus to another window dismisses (after the window had its first focus).
    let was_active = Rc::new(RefCell::new(false));
    {
        let finish = finish.clone();
        let was_active = was_active.clone();
        window.connect_is_active_notify(move |w| {
            if w.is_active() {
                *was_active.borrow_mut() = true;
            } else if *was_active.borrow() {
                let finish = finish.clone();
                glib::timeout_add_local_once(FOCUS_GRACE / 4, move || finish("none"));
            }
        });
    }

    // Closing the window by any other route still leaves an answer; the timeout dismisses.
    {
        let result = args.result.clone();
        let answered = answered.clone();
        window.connect_close_request(move |_| {
            if !answered.replace(true) {
                answer(&result, "none");
            }
            glib::Propagation::Proceed
        });
    }
    {
        let finish = finish.clone();
        glib::timeout_add_local_once(Duration::from_secs(args.timeout), move || finish("none"));
    }
    window.present();
}
