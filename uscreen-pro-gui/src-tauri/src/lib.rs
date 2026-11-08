//! Tauri backend of UScreen Pro: the IPC layer between the React UI and the host daemon.
//!
//! The GUI never touches video or input; it only manages things around the daemon — adb
//! devices ([`adb`]), wireless pairing ([`pairing`]), the daemon's lifecycle and control
//! socket ([`daemon`]), `config.toml` ([`config`]) and unpairing ([`forget`]). Every
//! `#[tauri::command]` registered in [`run`] is callable from the frontend via `invoke()`.

mod adb;
mod config;
mod daemon;
mod forget;
mod pairing;

/// Build and run the Tauri application: register the IPC commands that the React frontend invokes.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(pairing::PairingState::default())
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::save_config,
            config::save_device_profile,
            forget::forget_device,
            daemon::get_daemon_status,
            daemon::pause_device,
            daemon::resume_device,
            daemon::restart_session,
            daemon::queue_apk_install,
            daemon::launch_daemon,
            daemon::stop_daemon,
            daemon::get_autostart_status,
            daemon::set_autostart_status,
            adb::list_adb_devices,
            adb::wireless_device_info,
            adb::install_apk_on_device,
            adb::open_developer_options,
            adb::open_wireless_debugging_settings,
            adb::identify_device,
            adb::restart_adb_server,
            pairing::initiate_qr_pairing,
            pairing::cancel_qr_pairing,
            pairing::manual_adb_pair,
            pairing::manual_adb_connect,
            pairing::bootstrap_wireless_over_usb,
            pairing::pair_via_tailscale,
            pairing::detect_tailscale_ip,
        ])
        .run(tauri::generate_context!())
        .expect("error while running UScreen Pro application");
}
