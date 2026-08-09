use serde::Serialize;
use tauri::Emitter;

use crate::airplay::device::ConnectionState;
use crate::audio::capture::AudioDeviceInfo;
use crate::error::AppResult;

/// Emit a device-discovered event to the frontend
pub fn emit_device_discovered(
    app: &tauri::AppHandle,
    device: &(impl Serialize + Clone),
) -> AppResult<()> {
    app.emit("device-discovered", device.clone())
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit a device-lost event to the frontend
pub fn emit_device_lost(app: &tauri::AppHandle, device_id: &str) -> AppResult<()> {
    #[derive(Serialize, Clone)]
    struct DeviceLostPayload {
        device_id: String,
    }
    app.emit(
        "device-lost",
        DeviceLostPayload {
            device_id: device_id.to_string(),
        },
    )
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit a connection state change event
pub fn emit_connection_changed(
    app: &tauri::AppHandle,
    device_id: &str,
    state: &ConnectionState,
) -> AppResult<()> {
    #[derive(Serialize, Clone)]
    struct ConnectionPayload {
        device_id: String,
        state: ConnectionState,
    }
    app.emit(
        "connection-state-changed",
        ConnectionPayload {
            device_id: device_id.to_string(),
            state: state.clone(),
        },
    )
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit playback started event
pub fn emit_playback_started(app: &tauri::AppHandle, device_ids: &[String]) -> AppResult<()> {
    #[derive(Serialize, Clone)]
    struct PlaybackPayload {
        device_ids: Vec<String>,
    }
    app.emit(
        "playback-started",
        PlaybackPayload {
            device_ids: device_ids.to_vec(),
        },
    )
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit playback stopped event
pub fn emit_playback_stopped(app: &tauri::AppHandle, reason: &str) -> AppResult<()> {
    #[derive(Serialize, Clone)]
    struct PlaybackStopPayload {
        reason: String,
    }
    app.emit(
        "playback-stopped",
        PlaybackStopPayload {
            reason: reason.to_string(),
        },
    )
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit playback error event
pub fn emit_playback_error(app: &tauri::AppHandle, device_id: &str, error: &str) -> AppResult<()> {
    #[derive(Serialize, Clone)]
    struct PlaybackErrorPayload {
        device_id: String,
        error: String,
    }
    app.emit(
        "playback-error",
        PlaybackErrorPayload {
            device_id: device_id.to_string(),
            error: error.to_string(),
        },
    )
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit the Windows output endpoint currently used for loopback capture.
pub fn emit_audio_capture_changed(
    app: &tauri::AppHandle,
    device: &AudioDeviceInfo,
) -> AppResult<()> {
    app.emit("audio-capture-changed", device.clone())
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}

/// Emit volume changed event
pub fn emit_volume_changed(app: &tauri::AppHandle, device_id: &str, volume: f32) -> AppResult<()> {
    #[derive(Serialize, Clone)]
    struct VolumePayload {
        device_id: String,
        volume: f32,
    }
    app.emit(
        "volume-changed",
        VolumePayload {
            device_id: device_id.to_string(),
            volume,
        },
    )
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(())
}
