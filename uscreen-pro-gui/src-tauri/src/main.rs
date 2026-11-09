//! Binary entry point of the UScreen Pro desktop app; all logic lives in the library crate (`lib.rs`).

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Entry point of the desktop app.
fn main() {
    // WebKitGTK's accelerated compositing path renders blurry text under fractional scaling.
    // This must be set before Tauri/WebKit initialise, and while the process is still
    // single-threaded.
    unsafe { std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1") };
    uscreen_pro_gui_lib::run()
}
