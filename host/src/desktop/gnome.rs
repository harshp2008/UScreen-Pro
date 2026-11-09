//! GNOME Shell on Wayland. A Wayland client cannot ask the compositor which window has focus, so
//! the `uscreen-app-lock` Shell extension (see `extensions/`) reports it in `focus.json`; this
//! backend reads that file. It also asks the extension for the pointer context (`context_request` /
//! `context.json`) and places windows with the `spawn-at` command. Verified on GNOME 46 (Ubuntu 24.04) only.

use super::focus_file::FocusFile;
use super::{
    data_file, spawn_at, Capabilities, DesktopBackend, FocusUpdate, PointerContext, WindowRef,
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
    fn name(&self) -> &'static str {
        "gnome"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            focus: true,
            context: true,
            placement: spawn_at::installed(),
        }
    }

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

    fn focused_window(&mut self) -> Option<WindowRef> {
        spawn_at::focused_window()
    }

    fn focus_window(&mut self, window: &WindowRef) -> bool {
        spawn_at::focus_window(window)
    }

    fn spawn_at_pointer(&mut self, lift: i32, class: &str, command: &[String]) -> bool {
        spawn_at::spawn_at_pointer(lift, class, command)
    }
}
