use tauri::State;

use crate::airplay::device::AirPlayDevice;
use crate::airplay::discovery::DiscoveryService;
use crate::state::AppState;

/// Start scanning for AirPlay devices on the network
#[tauri::command]
pub async fn start_scan(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut active_service = state.discovery_service.lock().await;
    if active_service.is_some() {
        *state.scanning.lock().await = true;
        return Ok(());
    }

    let mut service = DiscoveryService::new().map_err(|e| e.to_string())?;

    service
        .start_browsing(app, state.device_registry.clone())
        .map_err(|e| e.to_string())?;

    *active_service = Some(service);
    *state.scanning.lock().await = true;
    Ok(())
}

/// Stop scanning for AirPlay devices
#[tauri::command]
pub async fn stop_scan(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(mut service) = state.discovery_service.lock().await.take() {
        service.stop_browsing().map_err(|error| error.to_string())?;
    }
    let mut scanning = state.scanning.lock().await;
    *scanning = false;
    Ok(())
}

/// Get the list of discovered devices
#[tauri::command]
pub async fn get_devices(state: State<'_, AppState>) -> Result<Vec<AirPlayDevice>, String> {
    let registry = state.device_registry.lock().await;
    Ok(registry.values().cloned().collect())
}
