//! The background thread that turns a long-press request into a menu on screen and, if a button is
//! chosen, the key combination in the focused app.
//!
//! Per request (the pen is still touching, its click held back): check the setting and the desktop's
//! abilities, skip art apps (holding still is normal while painting), ask the desktop what is under
//! the pointer, let `decision` pick the buttons, remember the focused window, show `uscreen-menu`
//! at the pointer, tell the router the menu is up (or that none applies, so the click goes out), wait
//! for the answer, give the window focus back, send the keys.

use super::actions::MenuAction;
use super::decision::{items, Rules};
use crate::desktop::{data_file, DesktopBackend, MenuRequest};
use crate::input::ghost::route_to_pen;
use crate::input::uinput::InjectDevices;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// How long to wait for the desktop's description of the pointer.
const CONTEXT_TIMEOUT: Duration = Duration::from_millis(250);
/// The menu closes itself after this long without a choice; the runner waits a little longer.
const MENU_TIMEOUT: Duration = Duration::from_secs(8);
/// The menu window's app id (set in `menu/src/main.rs`): the desktop backend needs it to find the window.
const MENU_CLASS: &str = "app.uscreen.menu";
/// Pixels between the cursor and the menu's centre line (the menu appears above the cursor).
const LIFT_PX: i32 = 64;
/// Pause between the menu window going away and the keys, so the app has focus again.
const FOCUS_SETTLE: Duration = Duration::from_millis(60);

/// Start the runner for one tablet: creates the request channel (the sender is stored in the
/// devices, where the long-press detection finds it) and the thread that answers requests.
pub(in crate::input) fn spawn(uinput: Arc<Mutex<InjectDevices>>) {
    let (tx, rx) = channel();
    if let Ok(mut g) = uinput.lock() {
        g.menu_tx = Some(tx);
    }
    let r = std::thread::Builder::new()
        .name("uscreen-menu".into())
        .spawn(move || run(uinput, rx));
    if let Err(e) = r {
        warn!("Could not start the pen menu thread: {}", e);
    }
}

/// The runner thread: answers each menu request (showing the menu and sending the chosen keys) until the channel closes.
fn run(uinput: Arc<Mutex<InjectDevices>>, rx: Receiver<()>) {
    let mut desktop = crate::desktop::detect();
    let caps = desktop.capabilities();
    info!(
        "Pen menu: desktop {} (context: {}, placement: {})",
        desktop.name(),
        caps.context,
        caps.placement
    );
    while rx.recv().is_ok() {
        // Requests that piled up while a menu was open are stale.
        while rx.try_recv().is_ok() {}
        handle_request(desktop.as_mut(), &uinput);
    }
}

/// Answer one long-press request. The pen is still touching the screen with its press held back, so
/// this must tell the router what happened: no menu applies (the press goes out and the hold goes on
/// as an ordinary click) or the menu is up (the rest of the touch is swallowed). The chosen action is
/// carried out once the menu closes.
fn handle_request(desktop: &mut dyn DesktopBackend, uinput: &Arc<Mutex<InjectDevices>>) {
    let Some(plan) = plan(desktop) else {
        resolve(uinput, false);
        return;
    };
    // The pen may have moved on (a drag) while the desktop was being asked.
    if !uinput.lock().is_ok_and(|g| g.hold_pending()) {
        resolve(uinput, false);
        return;
    }
    // Preferred: the desktop draws the menu itself (no window, no focus change).
    if desktop.capabilities().menu {
        let req = MenuRequest {
            id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1),
            items: plan.buttons.iter().map(|a| a.name().to_string()).collect(),
            style: plan.style.clone(),
            timeout: MENU_TIMEOUT,
        };
        if desktop.show_menu(&req) {
            resolve(uinput, true);
            let chosen = desktop
                .wait_menu_choice(&req)
                .and_then(|n| MenuAction::from_name(&n));
            send_keys(uinput, chosen);
            return;
        }
        debug!("Pen menu: the shell did not draw it, falling back to a window");
    }
    if !desktop.capabilities().placement {
        resolve(uinput, false);
        return;
    }
    let _ = std::fs::remove_file(&plan.result);
    let previous = desktop.focused_window();
    let command = menu_command(&plan.buttons, &plan.style, &plan.result);
    if !desktop.spawn_at_pointer(LIFT_PX, MENU_CLASS, &command) {
        warn!("Pen menu: could not show the menu window ({:?})", command.first());
        resolve(uinput, false);
        return;
    }
    resolve(uinput, true);
    let chosen = wait_for_choice(&plan.result);
    if let Some(w) = previous {
        desktop.focus_window(&w);
        std::thread::sleep(FOCUS_SETTLE);
    }
    send_keys(uinput, chosen);
}

/// Carry out the chosen action: its key combination, through the virtual keyboard.
fn send_keys(uinput: &Arc<Mutex<InjectDevices>>, chosen: Option<MenuAction>) {
    if let Some(action) = chosen {
        debug!("Pen menu: {}", action.name());
        if let Ok(mut g) = uinput.lock() {
            g.tap_key_combo(&action.keys());
        }
    }
}

/// Tell the router whether a menu is on screen (`true`) or not (`false`).
fn resolve(uinput: &Arc<Mutex<InjectDevices>>, shown: bool) {
    if let Ok(mut g) = uinput.lock() {
        g.auto_menu_resolved(shown);
    }
}

/// What to show: the buttons, their style and where the answer goes.
struct Plan {
    buttons: Vec<MenuAction>,
    style: String,
    result: std::path::PathBuf,
}

/// Decide whether a menu applies here: the setting, the desktop's abilities, an app that is not an
/// art app, and `decision::items` on what the desktop says is under the pointer.
fn plan(desktop: &mut dyn DesktopBackend) -> Option<Plan> {
    let cfg = crate::config::FileConfig::load();
    if !cfg.pen_menu {
        return None;
    }
    let caps = desktop.capabilities();
    if !(caps.context && (caps.menu || caps.placement)) {
        debug!("Pen menu: not available on {}", desktop.name());
        return None;
    }
    if route_to_pen(&cfg.stylus_art_apps) {
        return None;
    }
    let ctx = desktop.context_at_pointer(CONTEXT_TIMEOUT)?;
    let buttons = items(&ctx, &Rules::default());
    if buttons.is_empty() {
        return None;
    }
    Some(Plan { buttons, style: cfg.pen_menu_style, result: data_file("menu_result")? })
}

/// The command line that shows the menu window with these buttons.
fn menu_command(buttons: &[MenuAction], style: &str, result: &std::path::Path) -> Vec<String> {
    let names: Vec<&str> = buttons.iter().map(|a| a.name()).collect();
    vec![
        menu_binary(),
        "--items".into(),
        names.join(","),
        "--style".into(),
        style.into(),
        "--timeout".into(),
        MENU_TIMEOUT.as_secs().to_string(),
        "--result".into(),
        result.display().to_string(),
    ]
}

/// `uscreen-menu` next to the daemon binary if it is there, else whatever `$PATH` finds.
fn menu_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("uscreen-menu")))
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "uscreen-menu".into())
}

/// Wait for the menu to write its result file; the chosen action, `None` if it was dismissed.
fn wait_for_choice(result: &std::path::Path) -> Option<MenuAction> {
    let deadline = Instant::now() + MENU_TIMEOUT + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(result) {
            if text.ends_with('\n') {
                return parse_choice(&text);
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    None
}

/// The action named in the menu's result line (`{"action":"copy"}`); `None` for `none` or junk.
fn parse_choice(text: &str) -> Option<MenuAction> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    MenuAction::from_name(v.get("action")?.as_str()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_is_read_from_the_result_line() {
        assert_eq!(
            parse_choice("{\"action\":\"copy\"}\n"),
            Some(MenuAction::Copy)
        );
        assert_eq!(parse_choice("{\"action\":\"none\"}\n"), None);
        assert_eq!(parse_choice("junk"), None);
    }

    #[test]
    fn the_command_names_every_button_and_the_result_file() {
        let c = menu_command(
            &[MenuAction::Paste, MenuAction::Undo],
            "icons",
            std::path::Path::new("/tmp/r"),
        );
        assert!(c.windows(2).any(|w| w == ["--items", "paste,undo"]));
        assert!(c.windows(2).any(|w| w == ["--style", "icons"]));
        assert!(c.windows(2).any(|w| w == ["--result", "/tmp/r"]));
    }
}
