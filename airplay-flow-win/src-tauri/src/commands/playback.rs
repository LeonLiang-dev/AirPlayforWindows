use tauri::State;

use crate::airplay::device::ConnectionState;
use crate::airplay::session::SessionState;
use crate::audio::pipeline::{AudioPipeline, AudioPipelineEvent};
use crate::events::{
    emit_audio_capture_changed, emit_connection_changed, emit_playback_error,
    emit_playback_started, emit_playback_stopped, emit_volume_changed,
};
use crate::state::AppState;

#[tauri::command]
pub async fn start_streaming(
    device_ids: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if device_ids.is_empty() {
        return Err("Select at least one connected device".to_string());
    }

    let device_ids: Vec<String> = device_ids.into_iter().fold(Vec::new(), |mut ids, id| {
        if !ids.contains(&id) {
            ids.push(id);
        }
        ids
    });

    log::info!("Starting streaming to {} device(s)", device_ids.len());

    {
        let mut connections = state.active_connections.lock().await;

        for device_id in &device_ids {
            let session = connections
                .get(device_id)
                .ok_or_else(|| format!("Device not connected: {device_id}"))?;
            if !matches!(
                session.state,
                SessionState::SetupComplete | SessionState::Paused | SessionState::Recording
            ) {
                return Err(format!(
                    "Device {device_id} is not ready to stream (state: {:?})",
                    session.state
                ));
            }
        }

        for device_id in &device_ids {
            let session = connections
                .get_mut(device_id)
                .expect("connection was validated above");
            if session.state != SessionState::Recording {
                if let Err(error) = session.start_recording().await {
                    let message = error.to_string();
                    let _ = emit_playback_error(&app, device_id, &message);
                    return Err(message);
                }
            }
        }
    }

    let (pipeline_event_tx, mut pipeline_event_rx) = tokio::sync::mpsc::unbounded_channel();
    let pipeline_event_app = app.clone();
    let pipeline_event_device_ids = device_ids.clone();
    tokio::spawn(async move {
        while let Some(event) = pipeline_event_rx.recv().await {
            match event {
                AudioPipelineEvent::CaptureDeviceChanged(device) => {
                    if let Err(error) = emit_audio_capture_changed(&pipeline_event_app, &device) {
                        log::warn!("Unable to emit capture source change: {error}");
                    }
                }
                AudioPipelineEvent::CaptureError(error) => {
                    for device_id in &pipeline_event_device_ids {
                        if let Err(emit_error) =
                            emit_playback_error(&pipeline_event_app, device_id, &error)
                        {
                            log::warn!("Unable to emit capture error: {emit_error}");
                        }
                    }
                }
            }
        }
    });

    let pipeline_start = {
        let mut pipeline_guard = state.audio_pipeline.lock().await;
        let pipeline = pipeline_guard.get_or_insert_with(AudioPipeline::new);
        pipeline
            .start(
                state.active_connections.clone(),
                device_ids.clone(),
                Some(pipeline_event_tx),
            )
            .await
    };
    if let Err(error) = pipeline_start {
        let message = error.to_string();
        let _ = pause_sessions(&state).await;
        for device_id in &device_ids {
            let _ = emit_playback_error(&app, device_id, &message);
        }
        return Err(message);
    }

    update_device_states(&state, &app, &device_ids, ConnectionState::Streaming).await;
    if let Err(error) = emit_playback_started(&app, &device_ids) {
        log::warn!("Unable to emit playback start event: {error}");
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_streaming(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("Stopping audio streaming");
    stop_pipeline(&state).await?;

    let device_ids = pause_sessions(&state).await?;
    update_device_states(&state, &app, &device_ids, ConnectionState::Ready).await;
    if let Err(error) = emit_playback_stopped(&app, "stopped by user") {
        log::warn!("Unable to emit playback stop event: {error}");
    }
    Ok(())
}

#[tauri::command]
pub async fn set_volume(
    device_id: String,
    volume: f32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let normalized_volume = volume.clamp(0.0, 1.0);
    let mut connections = state.active_connections.lock().await;
    let session = connections
        .get_mut(&device_id)
        .ok_or_else(|| format!("Device not connected: {device_id}"))?;
    session
        .set_volume(normalized_volume)
        .await
        .map_err(|error| error.to_string())?;
    drop(connections);

    if let Err(error) = emit_volume_changed(&app, &device_id, normalized_volume) {
        log::warn!("Unable to emit volume event: {error}");
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_playback(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    stop_pipeline(&state).await?;
    let device_ids = pause_sessions(&state).await?;
    update_device_states(&state, &app, &device_ids, ConnectionState::Ready).await;
    if let Err(error) = emit_playback_stopped(&app, "paused by user") {
        log::warn!("Unable to emit playback pause event: {error}");
    }
    Ok(())
}

async fn stop_pipeline(state: &AppState) -> Result<(), String> {
    let mut pipeline = state.audio_pipeline.lock().await;
    if let Some(pipeline) = pipeline.as_mut() {
        pipeline.stop().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn pause_sessions(state: &AppState) -> Result<Vec<String>, String> {
    let mut connections = state.active_connections.lock().await;
    let device_ids = connections.keys().cloned().collect::<Vec<_>>();
    for session in connections.values_mut() {
        session.pause().await.map_err(|error| error.to_string())?;
    }
    Ok(device_ids)
}

async fn update_device_states(
    state: &AppState,
    app: &tauri::AppHandle,
    device_ids: &[String],
    connection_state: ConnectionState,
) {
    {
        let mut registry = state.device_registry.lock().await;
        for device_id in device_ids {
            if let Some(device) = registry.get_mut(device_id) {
                device.connection_state = connection_state.clone();
            }
        }
    }

    for device_id in device_ids {
        if let Err(error) = emit_connection_changed(app, device_id, &connection_state) {
            log::warn!("Unable to emit connection state change: {error}");
        }
    }
}
