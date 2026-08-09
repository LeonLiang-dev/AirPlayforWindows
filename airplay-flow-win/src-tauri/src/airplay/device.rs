use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

/// Represents the supported audio codecs for a device
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum CodecSupport {
    Pcm,
    Alac,
    Aac,
    AlacAndAac,
    #[default]
    Unknown,
}

/// Encryption type detected from device TXT record
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum EncryptionType {
    None,
    Rsa,
    FairPlay,
    #[default]
    Unknown,
}

/// Connection state machine for an AirPlay device
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionState {
    Discovered,
    Connecting,
    Paired,
    Ready,
    Streaming,
    Error(String),
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Discovered => write!(f, "discovered"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Paired => write!(f, "paired"),
            ConnectionState::Ready => write!(f, "ready"),
            ConnectionState::Streaming => write!(f, "streaming"),
            ConnectionState::Error(e) => write!(f, "error: {}", e),
        }
    }
}

/// Represents a discovered AirPlay device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirPlayDevice {
    /// Unique device identifier (from TXT record `deviceid` or generated)
    pub id: String,
    /// Human-readable device name (from mDNS service name)
    pub name: String,
    /// IP address of the device
    pub host: String,
    /// RTSP control port (typically 7000 for RAOP)
    pub port: u16,
    /// Protocol version from `fv` TXT field
    pub protocol_version: String,
    /// Supported features bitmask
    pub features: u64,
    /// Capability flags
    pub flags: u64,
    /// Device model string (e.g., "AppleTV6,2")
    pub model: String,
    /// Supported audio codecs
    pub codecs: CodecSupport,
    /// Encryption type required
    pub encryption: EncryptionType,
    /// Whether the receiver advertises encryption type 4 and expects /auth-setup.
    pub requires_auth_setup: bool,
    /// Whether we have persited pairing data
    pub paired: bool,
    /// Current connection state
    pub connection_state: ConnectionState,
    /// Ed25519 public key (base64, for AirPlay 2 pairing)
    pub public_key: Option<String>,
}

impl AirPlayDevice {
    /// Create a new device from mDNS discovery data
    pub fn new(id: String, name: String, host: Ipv4Addr, port: u16) -> Self {
        Self {
            id,
            name,
            host: host.to_string(),
            port,
            protocol_version: String::new(),
            features: 0,
            flags: 0,
            model: String::new(),
            codecs: CodecSupport::Unknown,
            encryption: EncryptionType::Unknown,
            requires_auth_setup: false,
            paired: false,
            connection_state: ConnectionState::Discovered,
            public_key: None,
        }
    }
}
