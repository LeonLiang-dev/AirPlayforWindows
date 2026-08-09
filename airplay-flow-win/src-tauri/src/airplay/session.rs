/// Session management for a single AirPlay device connection.
///
/// Manages the lifecycle: Connect -> Announce -> Setup -> Stream -> Teardown.
use log::{debug, info};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

use crate::airplay::device::{AirPlayDevice, ConnectionState};
use crate::airplay::rtp::{build_sync_packet, RtpStream};
use crate::airplay::rtsp::RtspClient;
use crate::airplay::sdp::StreamConfig;
use crate::airplay::timing::TimingResponder;
use crate::error::{AppError, AppResult};

const FIRST_CONTROL_PORT: u16 = 6001;
const LAST_CONTROL_PORT: u16 = 6099;
const DEFAULT_LATENCY_FRAMES: u32 = 11_025;
const SYNC_INTERVAL: Duration = Duration::from_secs(1);

/// Represents an active RTSP session with an AirPlay device
pub struct RtspSession {
    pub device_id: String,
    pub rtsp: RtspClient,
    pub rtp: Option<RtpStream>,
    pub state: SessionState,
    pub config: StreamConfig,
    pub client_control_port: u16,
    pub client_timing_port: u16,
    pub volume: f32,
    requires_auth_setup: bool,
    control_socket: Option<UdpSocket>,
    timing_responder: Option<TimingResponder>,
    latency_frames: u32,
    initial_sync_pending: bool,
    last_sync_at: Option<Instant>,
}

/// Session operational state
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Disconnected,
    Connected,
    Announced,
    SetupComplete,
    Recording,
    Paused,
    Error(String),
}

impl SessionState {
    pub fn as_connection_state(&self) -> ConnectionState {
        match self {
            SessionState::Disconnected => ConnectionState::Discovered,
            SessionState::Connected => ConnectionState::Connecting,
            SessionState::Announced => ConnectionState::Ready,
            SessionState::SetupComplete => ConnectionState::Ready,
            SessionState::Recording => ConnectionState::Streaming,
            SessionState::Paused => ConnectionState::Ready,
            SessionState::Error(e) => ConnectionState::Error(e.clone()),
        }
    }
}

impl RtspSession {
    /// Create a new session for a device
    pub fn new(device: &AirPlayDevice) -> Self {
        Self {
            device_id: device.id.clone(),
            rtsp: RtspClient::new(device.host.clone(), device.port),
            rtp: None,
            state: SessionState::Disconnected,
            config: StreamConfig::default(),
            client_control_port: 0,
            client_timing_port: 0,
            volume: 0.5,
            requires_auth_setup: device.requires_auth_setup,
            control_socket: None,
            timing_responder: None,
            latency_frames: DEFAULT_LATENCY_FRAMES,
            initial_sync_pending: true,
            last_sync_at: None,
        }
    }

    /// Execute the full connection handshake:
    /// TCP connect -> OPTIONS -> ANNOUNCE -> SETUP
    pub async fn connect(&mut self) -> AppResult<()> {
        info!("Connecting to device {}...", self.device_id);

        // 1. TCP Connect
        self.rtsp.connect()?;
        self.state = SessionState::Connected;

        // 2. OPTIONS
        let response = self
            .rtsp
            .options()
            .map_err(|error| rtsp_step_error("OPTIONS", error))?;
        if !response.status.is_ok() {
            return Err(AppError::RtspError(format!(
                "OPTIONS failed: {:?}",
                response.status
            )));
        }
        info!("OPTIONS OK for {}", self.device_id);

        // Receivers advertising et=4 require the MFi auth-setup exchange even
        // when et=0 also allows a clear audio stream.
        if self.requires_auth_setup {
            let response = self
                .rtsp
                .auth_setup()
                .map_err(|error| rtsp_step_error("AUTH_SETUP", error))?;
            if !response.status.is_ok() {
                return Err(AppError::RtspError(format!(
                    "AUTH_SETUP failed: {:?}",
                    response.status
                )));
            }
            info!("AUTH_SETUP OK for {}", self.device_id);
        }

        // The timing responder must be running before ANNOUNCE/SETUP. Some
        // receivers wait for timing replies before completing SETUP.
        let control_socket = bind_control_socket().await?;
        let timing_responder = TimingResponder::bind()?;
        self.client_control_port = control_socket
            .local_addr()
            .map_err(|error| AppError::StreamError(error.to_string()))?
            .port();
        self.client_timing_port = timing_responder.local_port();
        self.control_socket = Some(control_socket);
        self.timing_responder = Some(timing_responder);

        // 3. ANNOUNCE (describe the stream)
        let response = self
            .rtsp
            .announce(&self.config)
            .map_err(|error| rtsp_step_error("ANNOUNCE", error))?;
        if !response.status.is_ok() {
            return Err(AppError::RtspError(format!(
                "ANNOUNCE failed: {:?}",
                response.status
            )));
        }
        self.state = SessionState::Announced;
        info!("ANNOUNCE OK for {}", self.device_id);

        // 4. SETUP (negotiate transport).
        let response = self
            .rtsp
            .setup(self.client_control_port, self.client_timing_port)
            .map_err(|error| rtsp_step_error("SETUP", error))?;
        if !response.status.is_ok() {
            return Err(AppError::RtspError(format!(
                "SETUP failed: {:?}",
                response.status
            )));
        }
        self.state = SessionState::SetupComplete;
        info!("SETUP OK for {}", self.device_id);

        // 5. Create RTP stream
        let server_port = self.rtsp.server_port.unwrap_or(6001);
        let remote_addr: std::net::SocketAddr = format!("{}:{}", self.rtsp.host, server_port)
            .parse()
            .map_err(|e: std::net::AddrParseError| AppError::StreamError(e.to_string()))?;

        self.rtp = Some(
            RtpStream::new(
                remote_addr,
                self.config.payload_type,
                self.config.sample_rate,
            )
            .await?,
        );

        let server_control_port = self.rtsp.control_port().ok_or_else(|| {
            AppError::StreamError("SETUP response did not include a control_port".into())
        })?;
        let remote_control_addr: std::net::SocketAddr =
            format!("{}:{}", self.rtsp.host, server_control_port)
                .parse()
                .map_err(|e: std::net::AddrParseError| {
                    AppError::StreamError(format!("Invalid RAOP control address: {e}"))
                })?;
        self.control_socket
            .as_ref()
            .ok_or_else(|| AppError::StreamError("RAOP control socket is not initialized".into()))?
            .connect(remote_control_addr)
            .await
            .map_err(|error| {
                AppError::StreamError(format!("Connect RAOP control socket failed: {error}"))
            })?;
        info!(
            "RAOP control channel connected: local={}, remote={remote_control_addr}",
            self.client_control_port
        );

        info!("Session established with {}", self.device_id);
        Ok(())
    }

    /// Start audio recording (begin sending RTP)
    pub async fn start_recording(&mut self) -> AppResult<()> {
        if self.state != SessionState::SetupComplete && self.state != SessionState::Paused {
            return Err(AppError::RtspError(format!(
                "Cannot start recording from state {:?}",
                self.state
            )));
        }

        let (sequence_number, rtp_timestamp) = {
            let rtp = self
                .rtp
                .as_mut()
                .ok_or_else(|| AppError::StreamError("RTP stream not initialized".into()))?;
            rtp.prepare_recording();
            (rtp.sequence_number(), rtp.timestamp())
        };
        let response = self.rtsp.record(sequence_number, rtp_timestamp)?;
        if !response.status.is_ok() {
            return Err(AppError::RtspError(format!(
                "RECORD failed: {:?}",
                response.status
            )));
        }

        self.latency_frames = response
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("audio-latency"))
            .and_then(|(_, value)| value.parse::<u32>().ok())
            .map(|latency| latency.max(DEFAULT_LATENCY_FRAMES))
            .unwrap_or(DEFAULT_LATENCY_FRAMES);
        self.initial_sync_pending = true;
        self.last_sync_at = None;

        self.state = SessionState::Recording;
        debug!(
            "RECORD OK, streaming started with {} frame latency",
            self.latency_frames
        );
        Ok(())
    }

    /// Send an audio frame to the device
    pub async fn send_audio(&mut self, alac_frame: &[u8]) -> AppResult<()> {
        let media_timestamp = self
            .rtp
            .as_ref()
            .map(RtpStream::media_timestamp)
            .ok_or_else(|| AppError::StreamError("RTP stream not initialized".into()))?;
        let sync_due = self.initial_sync_pending
            || self
                .last_sync_at
                .map(|sent_at| sent_at.elapsed() >= SYNC_INTERVAL)
                .unwrap_or(true);
        if sync_due {
            self.send_sync(media_timestamp, self.initial_sync_pending)
                .await?;
            self.initial_sync_pending = false;
            self.last_sync_at = Some(Instant::now());
        }

        let rtp = self
            .rtp
            .as_mut()
            .ok_or_else(|| AppError::StreamError("RTP stream not initialized".into()))?;

        rtp.send_frame(alac_frame, self.config.frame_size).await
    }

    async fn send_sync(&self, media_timestamp: u64, first: bool) -> AppResult<()> {
        let socket = self
            .control_socket
            .as_ref()
            .ok_or_else(|| AppError::SyncError("RAOP control socket is not initialized".into()))?;
        let packet = build_sync_packet(
            media_timestamp,
            self.config.sample_rate,
            self.latency_frames,
            first,
        );
        let sent = socket
            .send(&packet)
            .await
            .map_err(|error| AppError::SyncError(format!("Send RAOP sync failed: {error}")))?;
        if sent != packet.len() {
            return Err(AppError::SyncError(format!(
                "Short RAOP sync packet: {sent}/{} bytes",
                packet.len()
            )));
        }
        debug!(
            "Sent {} RAOP sync at RTP timestamp {}",
            if first { "initial" } else { "periodic" },
            media_timestamp as u32
        );
        Ok(())
    }

    /// Pause the stream
    pub async fn pause(&mut self) -> AppResult<()> {
        if self.state != SessionState::Recording {
            return Ok(());
        }

        let response = self.rtsp.pause()?;
        if response.status.is_ok() {
            self.state = SessionState::Paused;
            debug!("Stream paused");
        }

        Ok(())
    }

    /// Set volume on the device
    pub async fn set_volume(&mut self, volume: f32) -> AppResult<()> {
        self.volume = volume.clamp(0.0, 1.0);
        let response = self.rtsp.set_volume(self.volume)?;
        if !response.status.is_ok() {
            info!("SET_PARAMETER volume failed: {:?}", response.status);
        }
        Ok(())
    }

    /// Flush the timing (reset sequence numbers)
    pub async fn flush(&mut self) -> AppResult<()> {
        let (sequence_number, rtp_timestamp) = self
            .rtp
            .as_ref()
            .map(|rtp| (rtp.sequence_number(), rtp.timestamp()))
            .unwrap_or((0, 0));
        let response = self.rtsp.flush(sequence_number, rtp_timestamp)?;
        if response.status.is_ok() {
            if let Some(rtp) = self.rtp.as_mut() {
                rtp.reset_sequence();
            }
            self.initial_sync_pending = true;
            self.last_sync_at = None;
            debug!("Stream flushed");
        }
        Ok(())
    }

    /// Tear down the session
    pub async fn teardown(&mut self) -> AppResult<()> {
        if matches!(
            self.state,
            SessionState::Disconnected | SessionState::Error(_)
        ) {
            return Ok(());
        }

        let _ = self.rtsp.teardown();
        self.rtsp.close();
        self.rtp = None;
        self.control_socket = None;
        self.timing_responder = None;
        self.initial_sync_pending = true;
        self.last_sync_at = None;
        self.state = SessionState::Disconnected;
        info!("Session torn down for {}", self.device_id);
        Ok(())
    }
}

fn rtsp_step_error(step: &str, error: AppError) -> AppError {
    let detail = match error {
        AppError::RtspError(detail) => detail,
        other => other.to_string(),
    };
    AppError::RtspError(format!("{step} request failed: {detail}"))
}

async fn bind_control_socket() -> AppResult<UdpSocket> {
    for port in FIRST_CONTROL_PORT..=LAST_CONTROL_PORT {
        match UdpSocket::bind(("0.0.0.0", port)).await {
            Ok(socket) => return Ok(socket),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(error) => {
                return Err(AppError::StreamError(format!(
                    "Bind control port {port} failed: {error}"
                )))
            }
        }
    }

    Err(AppError::StreamError(format!(
        "No free RAOP control port in {FIRST_CONTROL_PORT}..={LAST_CONTROL_PORT}"
    )))
}

#[cfg(test)]
mod tests {
    use super::{RtspSession, SessionState};
    use crate::airplay::device::AirPlayDevice;
    use crate::audio::pipeline::AudioPipeline;
    use std::collections::HashMap;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    #[tokio::test]
    #[ignore = "requires AIRPLAY_TEST_RECEIVER and a real RAOP receiver"]
    async fn connects_to_real_receiver_from_environment() {
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Debug)
            .try_init();
        let endpoint = std::env::var("AIRPLAY_TEST_RECEIVER")
            .expect("set AIRPLAY_TEST_RECEIVER to an IPv4 address, optionally followed by :port");
        let (host, port) = endpoint
            .split_once(':')
            .map(|(host, port)| (host, port.parse::<u16>().unwrap()))
            .unwrap_or((&endpoint, 7000));
        let host = host.parse::<Ipv4Addr>().unwrap();
        let mut device = AirPlayDevice::new(
            "integration-test".to_string(),
            "Integration test receiver".to_string(),
            host,
            port,
        );
        device.requires_auth_setup = true;

        let mut session = RtspSession::new(&device);
        session.connect().await.unwrap();
        assert_eq!(session.state, SessionState::SetupComplete);
        session.teardown().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires AIRPLAY_TEST_RECEIVER and a real RAOP receiver"]
    async fn streams_windows_loopback_to_real_receiver() {
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Info)
            .try_init();
        let endpoint = std::env::var("AIRPLAY_TEST_RECEIVER")
            .expect("set AIRPLAY_TEST_RECEIVER to an IPv4 address, optionally followed by :port");
        let (host, port) = endpoint
            .split_once(':')
            .map(|(host, port)| (host, port.parse::<u16>().unwrap()))
            .unwrap_or((&endpoint, 7000));
        let host = host.parse::<Ipv4Addr>().unwrap();
        let device_id = "loopback-integration-test".to_string();
        let mut device = AirPlayDevice::new(
            device_id.clone(),
            "Loopback integration test receiver".to_string(),
            host,
            port,
        );
        device.requires_auth_setup = true;

        let mut session = RtspSession::new(&device);
        session.connect().await.unwrap();
        session.start_recording().await.unwrap();

        let sessions = Arc::new(Mutex::new(HashMap::from([(device_id.clone(), session)])));
        let mut pipeline = AudioPipeline::new();
        pipeline
            .start(sessions.clone(), vec![device_id.clone()], None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        pipeline.stop().await.unwrap();

        let mut session = sessions.lock().await.remove(&device_id).unwrap();
        session.teardown().await.unwrap();
    }
}
