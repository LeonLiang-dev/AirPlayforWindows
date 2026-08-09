use tauri::State;

use crate::airplay::device::ConnectionState;
use crate::airplay::session::RtspSession;
use crate::events::emit_connection_changed;
use crate::state::AppState;

/// Connect to an AirPlay device and establish a streaming session
#[tauri::command]
pub async fn connect_device(
    device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state
        .active_connections
        .lock()
        .await
        .contains_key(&device_id)
    {
        update_connection_state(&state, &app, &device_id, ConnectionState::Ready).await;
        return Ok(());
    }

    let device = {
        let registry = state.device_registry.lock().await;
        registry
            .get(&device_id)
            .cloned()
            .ok_or_else(|| format!("Device not found: {}", device_id))?
    };

    update_connection_state(&state, &app, &device_id, ConnectionState::Connecting).await;

    let mut session = RtspSession::new(&device);
    if let Err(error) = session.connect().await {
        let message = error.to_string();
        log::warn!("Connection to {} failed: {}", device.name, message);
        update_connection_state(
            &state,
            &app,
            &device_id,
            ConnectionState::Error(message.clone()),
        )
        .await;
        return Err(message);
    }

    // Store the active session
    state
        .active_connections
        .lock()
        .await
        .insert(device_id.clone(), session);
    update_connection_state(&state, &app, &device_id, ConnectionState::Ready).await;

    log::info!("Connected to device: {}", device.name);
    Ok(())
}

/// Disconnect from an AirPlay device
#[tauri::command]
pub async fn disconnect_device(
    device_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(mut session) = state.active_connections.lock().await.remove(&device_id) {
        if let Err(error) = session.teardown().await {
            let message = error.to_string();
            update_connection_state(
                &state,
                &app,
                &device_id,
                ConnectionState::Error(message.clone()),
            )
            .await;
            return Err(message);
        }
    }
    update_connection_state(&state, &app, &device_id, ConnectionState::Discovered).await;
    Ok(())
}

async fn update_connection_state(
    state: &AppState,
    app: &tauri::AppHandle,
    device_id: &str,
    connection_state: ConnectionState,
) {
    let updated = {
        let mut registry = state.device_registry.lock().await;
        registry.get_mut(device_id).map(|device| {
            device.connection_state = connection_state.clone();
        })
    };

    if updated.is_some() {
        if let Err(error) = emit_connection_changed(app, device_id, &connection_state) {
            log::warn!("Unable to emit connection state change: {error}");
        }
    }
}
