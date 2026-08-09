use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, Clone)]
pub enum AppError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("RTSP error: {0}")]
    RtspError(String),

    #[error("Audio capture error: {0}")]
    CaptureError(String),

    #[error("Encoding error: {0}")]
    EncodeError(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Discovery error: {0}")]
    DiscoveryError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

// Allow AppError to be returned from Tauri commands
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

pub type AppResult<T> = Result<T, AppError>;
