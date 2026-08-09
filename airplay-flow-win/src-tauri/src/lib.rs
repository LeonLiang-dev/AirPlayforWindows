pub mod airplay;
pub mod audio;
pub mod commands;
pub mod error;
pub mod events;
pub mod state;
pub mod sync;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState::new())
        .setup(|_app| Ok(()))
        .invoke_handler(tauri::generate_handler![
            commands::discovery::start_scan,
            commands::discovery::stop_scan,
            commands::discovery::get_devices,
            commands::device::connect_device,
            commands::device::disconnect_device,
            commands::playback::start_streaming,
            commands::playback::stop_streaming,
            commands::playback::set_volume,
            commands::playback::pause_playback,
            commands::system::get_audio_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
