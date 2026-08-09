/// RAOP RTSP client implementation
///
/// Implements the RTSP control channel for AirPlay audio streaming.
/// Handles OPTIONS, ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN requests.
use log::{debug, info};
use rand::RngCore;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::airplay::crypto::x25519_public_key;
use crate::airplay::sdp::{self, StreamConfig};
use crate::error::{AppError, AppResult};

/// RTSP method/response parsing
#[derive(Debug, PartialEq)]
pub enum RtspStatus {
    Ok,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    InternalServerError,
    Other(u16),
}

impl RtspStatus {
    pub fn from_code(code: u16) -> Self {
        match code {
            200 => RtspStatus::Ok,
            401 => RtspStatus::Unauthorized,
            403 => RtspStatus::Forbidden,
            404 => RtspStatus::NotFound,
            405 => RtspStatus::MethodNotAllowed,
            500 => RtspStatus::InternalServerError,
            _ => RtspStatus::Other(code),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, RtspStatus::Ok)
    }
}

/// RTSP response
#[derive(Debug)]
pub struct RtspResponse {
    pub status: RtspStatus,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// RTSP client for RAOP communication
pub struct RtspClient {
    stream: Option<TcpStream>,
    pub host: String,
    pub port: u16,
    cseq: u32,
    session_id: Option<String>,
    pub server_port: Option<u16>,
    control_port: Option<u16>,
    timing_port: Option<u16>,
    stream_id: u32,
}

impl RtspClient {
    /// Create a new RTSP client
    pub fn new(host: String, port: u16) -> Self {
        Self {
            stream: None,
            host,
            port,
            cseq: 1,
            session_id: None,
            server_port: None,
            control_port: None,
            timing_port: None,
            stream_id: rand::random::<u32>(),
        }
    }

    /// Connect to the RTSP server
    pub fn connect(&mut self) -> AppResult<()> {
        let addr: SocketAddr = format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| AppError::RtspError(format!("Invalid address: {}", e)))?;

        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| AppError::RtspError(format!("Connection failed: {}", e)))?;

        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| AppError::RtspError(format!("Set timeout failed: {}", e)))?;

        self.stream = Some(stream);
        info!("RTSP connected to {}:{}", self.host, self.port);
        Ok(())
    }

    /// Send OPTIONS request and get supported methods
    pub fn options(&mut self) -> AppResult<RtspResponse> {
        let cseq = self.next_cseq();
        let request = format!(
            "OPTIONS * RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: AirPlayFlowWin/1.0\r\n\r\n",
            cseq
        );
        self.send_request(&request)
    }

    /// Initialize MFi authentication for receivers advertising `et=4`.
    pub fn auth_setup(&mut self) -> AppResult<RtspResponse> {
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        let public_key = x25519_public_key(&secret);
        let mut body = Vec::with_capacity(33);
        body.push(0x01);
        body.extend_from_slice(&public_key);

        let cseq = self.next_cseq();
        let headers = format!(
            "POST /auth-setup RTSP/1.0\r\n\
             CSeq: {cseq}\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Length: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n",
            body.len()
        );
        let mut request = headers.into_bytes();
        request.extend_from_slice(&body);

        debug!("AUTH_SETUP request: {} byte public-key payload", body.len());
        self.send_request_bytes(&request)
    }

    /// Send ANNOUNCE with SDP describing the audio stream
    pub fn announce(&mut self, config: &StreamConfig) -> AppResult<RtspResponse> {
        let local_ip = self
            .stream
            .as_ref()
            .ok_or_else(|| AppError::RtspError("Not connected".into()))?
            .local_addr()
            .map_err(|error| AppError::RtspError(format!("Read local address failed: {error}")))?
            .ip()
            .to_string();
        let sdp_body = sdp::build_sdp(config, &local_ip, &self.host, self.stream_id);
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;

        let request = format!(
            "ANNOUNCE rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n\
             {}",
            host,
            port,
            stream_id,
            cseq,
            sdp_body.len(),
            sdp_body
        );

        debug!("ANNOUNCE SDP:\n{}", sdp_body);
        self.send_request(&request)
    }

    /// Send SETUP to negotiate transport parameters
    pub fn setup(
        &mut self,
        client_control_port: u16,
        client_timing_port: u16,
    ) -> AppResult<RtspResponse> {
        let transport = format!(
            "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={};timing_port={}",
            client_control_port, client_timing_port
        );
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;

        let request = format!(
            "SETUP rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Transport: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n",
            host, port, stream_id, cseq, transport
        );

        debug!("SETUP request:\n{}", request);
        let response = self.send_request(&request)?;

        // Parse session ID and server ports from response
        for (key, value) in &response.headers {
            match key.to_lowercase().as_str() {
                "session" => {
                    self.session_id = Some(value.clone());
                }
                "transport" => {
                    parse_transport_header(
                        value,
                        &mut self.server_port,
                        &mut self.control_port,
                        &mut self.timing_port,
                    );
                }
                _ => {}
            }
        }

        Ok(response)
    }

    /// Send RECORD to start streaming
    pub fn record(&mut self, sequence_number: u16, rtp_timestamp: u32) -> AppResult<RtspResponse> {
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;
        let session = self.session_id.as_deref().unwrap_or("");
        let rtp_info = format!("seq={sequence_number};rtptime={rtp_timestamp}");

        let request = format!(
            "RECORD rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Session: {}\r\n\
             Range: npt=0-\r\n\
             RTP-Info: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n",
            host, port, stream_id, cseq, session, rtp_info
        );

        debug!("RECORD request:\n{}", request);
        self.send_request(&request)
    }

    /// Send PAUSE to pause streaming
    pub fn pause(&mut self) -> AppResult<RtspResponse> {
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;
        let session = self.session_id.as_deref().unwrap_or("");

        let request = format!(
            "PAUSE rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Session: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n",
            host, port, stream_id, cseq, session
        );

        debug!("PAUSE request:\n{}", request);
        self.send_request(&request)
    }

    /// Send FLUSH to reset timing
    pub fn flush(&mut self, sequence_number: u16, rtp_timestamp: u32) -> AppResult<RtspResponse> {
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;
        let session = self.session_id.as_deref().unwrap_or("");
        let rtp_info = format!("seq={sequence_number};rtptime={rtp_timestamp}");

        let request = format!(
            "FLUSH rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Session: {}\r\n\
             RTP-Info: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n",
            host, port, stream_id, cseq, session, rtp_info
        );

        debug!("FLUSH request:\n{}", request);
        self.send_request(&request)
    }

    /// Send TEARDOWN to end the session
    pub fn teardown(&mut self) -> AppResult<RtspResponse> {
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;
        let session = self.session_id.as_deref().unwrap_or("");

        let request = format!(
            "TEARDOWN rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Session: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n",
            host, port, stream_id, cseq, session
        );

        debug!("TEARDOWN request:\n{}", request);
        self.send_request(&request)
    }

    /// Send SET_PARAMETER to adjust volume
    pub fn set_volume(&mut self, volume: f32) -> AppResult<RtspResponse> {
        let cseq = self.next_cseq();
        let host = &self.host;
        let port = self.port;
        let stream_id = self.stream_id;
        let session = self.session_id.as_deref().unwrap_or("");
        let volume_db = linear_to_db(volume);
        let body = format!("volume: {:.6}\r\n", volume_db);

        let request = format!(
            "SET_PARAMETER rtsp://{}:{}/{} RTSP/1.0\r\n\
             CSeq: {}\r\n\
             Session: {}\r\n\
             Content-Type: text/parameters\r\n\
             Content-Length: {}\r\n\
             User-Agent: AirPlayFlowWin/1.0\r\n\
             \r\n\
             {}",
            host,
            port,
            stream_id,
            cseq,
            session,
            body.len(),
            body
        );

        debug!("SET_PARAMETER (volume) request:\n{}", request);
        self.send_request(&request)
    }

    /// Get whether we have an active session
    pub fn has_session(&self) -> bool {
        self.session_id.is_some()
    }

    /// UDP control port advertised by the receiver in the SETUP response.
    pub fn control_port(&self) -> Option<u16> {
        self.control_port
    }

    /// Close the connection
    pub fn close(&mut self) {
        self.stream = None;
        self.session_id = None;
    }

    // --- private helpers ---

    fn next_cseq(&mut self) -> u32 {
        let seq = self.cseq;
        self.cseq += 1;
        seq
    }

    fn send_request(&mut self, request: &str) -> AppResult<RtspResponse> {
        self.send_request_bytes(request.as_bytes())
    }

    fn send_request_bytes(&mut self, request: &[u8]) -> AppResult<RtspResponse> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| AppError::RtspError("Not connected".into()))?;

        // Send the request
        stream
            .write_all(request)
            .map_err(|e| AppError::RtspError(format!("Send failed: {}", e)))?;

        // Read the response
        Self::read_response(stream)
    }

    fn read_response(stream: &mut TcpStream) -> AppResult<RtspResponse> {
        let mut reader = BufReader::new(stream);

        // Read status line
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .map_err(|e| AppError::RtspError(format!("Read status line failed: {}", e)))?;

        debug!("RTSP status: {}", status_line.trim());

        // Parse status code
        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(AppError::RtspError(format!(
                "Invalid RTSP status line: {}",
                status_line.trim()
            )));
        }

        let status_code: u16 = parts[1]
            .parse()
            .map_err(|_| AppError::RtspError(format!("Invalid status code: {}", parts[1])))?;

        // Read headers
        let mut headers = Vec::new();
        let mut content_length: usize = 0;

        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| AppError::RtspError(format!("Read header failed: {}", e)))?;

            let trimmed = line.trim();
            if trimmed.is_empty() {
                break; // End of headers
            }

            if let Some((key, value)) = parse_header(trimmed) {
                if key.to_lowercase() == "content-length" {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.push((key, value));
            }
        }

        // Read body if present
        let body = if content_length > 0 {
            let mut buf = vec![0u8; content_length];
            use std::io::Read;
            reader
                .read_exact(&mut buf)
                .map_err(|e| AppError::RtspError(format!("Read body failed: {}", e)))?;
            Some(String::from_utf8_lossy(&buf).to_string())
        } else {
            None
        };

        Ok(RtspResponse {
            status: RtspStatus::from_code(status_code),
            headers,
            body,
        })
    }
}

/// Parse an HTTP/RTSP header line "Key: Value"
fn parse_header(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(2, ':');
    let key = parts.next()?.trim().to_string();
    let value = parts.next().unwrap_or("").trim().to_string();
    Some((key, value))
}

/// Parse transport header for server ports
fn parse_transport_header(
    transport: &str,
    server_port: &mut Option<u16>,
    control_port: &mut Option<u16>,
    timing_port: &mut Option<u16>,
) {
    for part in transport.split(';') {
        let part = part.trim();
        if part.starts_with("server_port=") {
            if let Some(port_str) = part.strip_prefix("server_port=") {
                *server_port = port_str.parse().ok();
            }
        } else if part.starts_with("control_port=") {
            if let Some(port_str) = part.strip_prefix("control_port=") {
                *control_port = port_str.parse().ok();
            }
        } else if part.starts_with("timing_port=") {
            if let Some(port_str) = part.strip_prefix("timing_port=") {
                *timing_port = port_str.parse().ok();
            }
        }
    }
}

/// Convert linear volume (0.0 to 1.0) to dB attenuation
fn linear_to_db(volume: f32) -> f32 {
    if volume <= 0.0 {
        -144.0
    } else if volume >= 1.0 {
        0.0
    } else {
        20.0 * volume.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::{linear_to_db, parse_header, parse_transport_header};

    #[test]
    fn parses_transport_ports() {
        let mut server = None;
        let mut control = None;
        let mut timing = None;
        parse_transport_header(
            "RTP/AVP/UDP;server_port=6001;control_port=6002;timing_port=6003",
            &mut server,
            &mut control,
            &mut timing,
        );
        assert_eq!(server, Some(6001));
        assert_eq!(control, Some(6002));
        assert_eq!(timing, Some(6003));
    }

    #[test]
    fn parses_headers_with_colons_in_values() {
        assert_eq!(
            parse_header("Location: rtsp://192.168.1.2:7000/stream"),
            Some((
                "Location".to_string(),
                "rtsp://192.168.1.2:7000/stream".to_string(),
            ))
        );
    }

    #[test]
    fn maps_linear_volume_to_decibels() {
        assert_eq!(linear_to_db(0.0), -144.0);
        assert_eq!(linear_to_db(1.0), 0.0);
        assert!((linear_to_db(0.5) + 6.0206).abs() < 0.001);
    }
}
