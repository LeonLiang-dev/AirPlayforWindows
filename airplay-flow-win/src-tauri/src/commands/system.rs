use crate::audio::capture::AudioDeviceInfo;

/// Get available audio output devices for capture
#[tauri::command]
pub async fn get_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    tauri::async_runtime::spawn_blocking(crate::audio::capture::enumerate_output_devices)
        .await
        .map_err(|error| format!("Unable to query Windows audio devices: {error}"))?
        .map_err(|error| error.to_string())
}
