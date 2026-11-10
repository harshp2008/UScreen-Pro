//! GNOME Shell on Wayland. A Wayland client cannot ask the compositor which window has focus, so
//! the `uscreen-app-lock` Shell extension (see `extensions/`) reports it in `focus.json`; this
//! backend reads that file. It also asks the extension for the pointer context (`context_request` /
//! `context.json`) and places windows with the `spawn-at` command. Verified on GNOME 46 (Ubuntu 24.04) only.

pub mod display;
pub mod spawn_at;

use super::focus_file::FocusFile;
use super::{
    data_file, Capabilities, DesktopBackend, FocusUpdate, MenuRequest, PointerContext, WindowRef,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The GNOME backend: focus comes from the extension's file.
pub(super) struct GnomeDesktop {
    focus: FocusFile,
}

impl GnomeDesktop {
    /// A backend that has not seen a focus change yet.
    pub(super) fn new() -> Self {
        Self {
            focus: FocusFile::default(),
        }
    }
}

impl DesktopBackend for GnomeDesktop {
    /// Backend name shown in logs.
    fn name(&self) -> &'static str {
        "gnome"
    }

    /// Focus, pointer context and the pen menu are available; placement only when the spawn-at tool is installed.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            focus: true,
            context: true,
            placement: spawn_at::installed(),
            menu: true,
        }
    }

    /// Focus comes from the Shell extension's focus poll.
    fn poll_focus(&mut self) -> FocusUpdate {
        self.focus.poll()
    }

    /// Ask the Shell extension: write `context_request`, then wait for `context.json` to carry a
    /// timestamp from after the request.
    fn context_at_pointer(&mut self, timeout: Duration) -> Option<PointerContext> {
        let request = data_file("context_request")?;
        let answer = data_file("context.json")?;
        let asked_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_micros() as i64;
        std::fs::write(&request, asked_us.to_string()).ok()?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(ctx) = std::fs::read_to_string(&answer)
                .ok()
                .and_then(|t| PointerContext::from_json(&t))
            {
                if ctx.t >= asked_us {
                    return Some(ctx);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    /// Ask the Shell extension to draw the menu: `menu_request.json` in, `menu_shown` (the request id)
    /// back as soon as it is on screen. An extension without the menu code never answers, and the
    /// caller falls back to a window.
    fn show_menu(&mut self, req: &MenuRequest) -> bool {
        let (Some(request), Some(shown), Some(result)) = (
            data_file("menu_request.json"),
            data_file("menu_shown"),
            data_file("menu_result"),
        ) else {
            return false;
        };
        let _ = std::fs::remove_file(&shown);
        let _ = std::fs::remove_file(&result);
        let body = serde_json::json!({
            "id": req.id,
            "items": req.items,
            "style": req.style,
            "timeout_ms": req.timeout.as_millis() as u64,
        });
        if std::fs::write(&request, body.to_string()).is_err() {
            return false;
        }
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            if std::fs::read_to_string(&shown).is_ok_and(|t| t.trim() == req.id.to_string()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Waits for the extension to write the menu's result (`menu_result`) for this request and returns the chosen action name.
    /// `None` when the result is `none` (dismissed) or nothing arrives within the request timeout plus 2 s.
    /// When `close_now` turns true it asks the extension to close the menu by writing `menu_close`.
    fn wait_menu_choice(
        &mut self,
        req: &MenuRequest,
        close_now: &dyn Fn() -> bool,
    ) -> Option<String> {
        let result = data_file("menu_result")?;
        let mut closing = false;
        let deadline = Instant::now() + req.timeout + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Some(v) = std::fs::read_to_string(&result)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim()).ok())
            {
                if v.get("id").and_then(|i| i.as_u64()) == Some(req.id) {
                    return v
                        .get("action")
                        .and_then(|a| a.as_str())
                        .filter(|a| *a != "none")
                        .map(String::from);
                }
            }
            if !closing && close_now() {
                closing = true;
                if let Some(f) = data_file("menu_close") {
                    let _ = std::fs::write(f, req.id.to_string());
                }
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        None
    }

    /// The focused window, via spawn-at.
    fn focused_window(&mut self) -> Option<WindowRef> {
        spawn_at::focused_window()
    }

    /// Gives the window keyboard focus, via spawn-at; true on success.
    fn focus_window(&mut self, window: &WindowRef) -> bool {
        spawn_at::focus_window(window)
    }

    /// Opens `command` as a window of class `class` at the pointer, `lift` pixels higher, via spawn-at; true on success.
    fn spawn_at_pointer(&mut self, lift: i32, class: &str, command: &[String]) -> bool {
        spawn_at::spawn_at_pointer(lift, class, command)
    }
}
