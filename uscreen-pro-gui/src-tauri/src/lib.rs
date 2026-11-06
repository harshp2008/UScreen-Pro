mod adb;
mod config;
mod daemon;
mod pairing;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(pairing::PairingState::default())
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::save_config,
            config::save_device_profile,
            config::delete_device_profile,
            daemon::get_daemon_status,
            daemon::pause_device,
            daemon::resume_device,
            daemon::launch_daemon,
            daemon::stop_daemon,
            daemon::get_autostart_status,
            daemon::set_autostart_status,
            adb::list_adb_devices,
            adb::install_apk_on_device,
            adb::open_developer_options,
            adb::open_wireless_debugging_settings,
            adb::restart_adb_server,
            pairing::initiate_qr_pairing,
            pairing::cancel_qr_pairing,
            pairing::manual_adb_pair,
            pairing::manual_adb_connect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running UScreen Pro application");
}
