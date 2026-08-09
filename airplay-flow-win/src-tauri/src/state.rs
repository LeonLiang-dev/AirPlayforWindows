use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::airplay::device::AirPlayDevice;
use crate::airplay::discovery::DiscoveryService;
use crate::airplay::session::RtspSession;
use crate::audio::pipeline::AudioPipeline;
use crate::sync::engine::SyncEngine;

/// Global application state shared across all Tauri commands
pub struct AppState {
    /// Registry of discovered AirPlay devices (keyed by device ID)
    pub device_registry: Arc<Mutex<HashMap<String, AirPlayDevice>>>,
    /// Active RTSP connections (keyed by device ID)
    pub active_connections: Arc<Mutex<HashMap<String, RtspSession>>>,
    /// Audio pipeline (optional, created when streaming starts)
    pub audio_pipeline: Arc<Mutex<Option<AudioPipeline>>>,
    /// Multi-room sync engine
    pub sync_engine: Arc<SyncEngine>,
    /// Whether mDNS scanning is active
    pub scanning: Arc<Mutex<bool>>,
    /// Active mDNS daemon and its browse workers.
    pub discovery_service: Arc<Mutex<Option<DiscoveryService>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            device_registry: Arc::new(Mutex::new(HashMap::new())),
            active_connections: Arc::new(Mutex::new(HashMap::new())),
            audio_pipeline: Arc::new(Mutex::new(None)),
            sync_engine: Arc::new(SyncEngine::new()),
            scanning: Arc::new(Mutex::new(false)),
            discovery_service: Arc::new(Mutex::new(None)),
        }
    }
}
