//! RAOP timing responder.
//!
//! Some receivers send timing probes while processing `SETUP` and do not
//! finish the RTSP request until the sender answers them.

use log::{debug, info, warn};
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{AppError, AppResult};

const TIMING_PACKET_SIZE: usize = 32;
const TIMING_RESPONSE_TYPE: u8 = 0x53 | 0x80;
const NTP_UNIX_EPOCH_OFFSET: u64 = 2_208_988_800;
const FIRST_TIMING_PORT: u16 = 6002;
const LAST_TIMING_PORT: u16 = 6100;

pub(crate) struct TimingResponder {
    local_port: u16,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl TimingResponder {
    pub(crate) fn bind() -> AppResult<Self> {
        let socket = bind_preferred_port()?;
        let local_port = socket
            .local_addr()
            .map_err(|error| AppError::SyncError(format!("Read timing port failed: {error}")))?
            .port();
        socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .map_err(|error| AppError::SyncError(format!("Set timing timeout failed: {error}")))?;

        // RTSP currently uses a blocking TCP stream. Keep timing on a dedicated
        // thread so a blocked RTSP read can never starve the UDP responder.
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let worker = thread::Builder::new()
            .name(format!("raop-timing-{local_port}"))
            .spawn(move || {
                let mut request = [0u8; 64];
                while worker_running.load(Ordering::Relaxed) {
                    let (length, sender) = match socket.recv_from(&mut request) {
                        Ok(received) => received,
                        Err(error)
                            if matches!(
                                error.kind(),
                                ErrorKind::WouldBlock | ErrorKind::TimedOut
                            ) =>
                        {
                            continue;
                        }
                        Err(error) => {
                            warn!("RAOP timing socket stopped: {error}");
                            break;
                        }
                    };
                    let received_at = SystemTime::now();

                    let Some(response) =
                        build_timing_response(&request[..length], received_at, SystemTime::now())
                    else {
                        debug!(
                            "Ignoring invalid RAOP timing packet ({length} bytes) from {sender}"
                        );
                        continue;
                    };

                    match socket.send_to(&response, sender) {
                        Ok(sent) if sent == response.len() => {
                            debug!("Answered RAOP timing probe from {sender}");
                        }
                        Ok(sent) => warn!(
                            "Short RAOP timing response to {sender}: {sent}/{} bytes",
                            response.len()
                        ),
                        Err(error) => {
                            warn!("Unable to answer RAOP timing probe from {sender}: {error}")
                        }
                    }
                }
            })
            .map_err(|error| {
                AppError::SyncError(format!("Start timing responder failed: {error}"))
            })?;

        info!("RAOP timing responder listening on UDP port {local_port}");
        Ok(Self {
            local_port,
            running,
            worker: Some(worker),
        })
    }

    pub(crate) fn local_port(&self) -> u16 {
        self.local_port
    }
}

fn bind_preferred_port() -> AppResult<UdpSocket> {
    for port in FIRST_TIMING_PORT..=LAST_TIMING_PORT {
        match UdpSocket::bind(("0.0.0.0", port)) {
            Ok(socket) => return Ok(socket),
            Err(error) if error.kind() == ErrorKind::AddrInUse => continue,
            Err(error) => {
                return Err(AppError::SyncError(format!(
                    "Bind timing port {port} failed: {error}"
                )))
            }
        }
    }

    Err(AppError::SyncError(format!(
        "No free RAOP timing port in {FIRST_TIMING_PORT}..={LAST_TIMING_PORT}"
    )))
}

impl Drop for TimingResponder {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn build_timing_response(
    request: &[u8],
    received_at: SystemTime,
    sent_at: SystemTime,
) -> Option<[u8; TIMING_PACKET_SIZE]> {
    if request.len() < TIMING_PACKET_SIZE {
        return None;
    }

    let receive_timestamp = ntp_timestamp(received_at)?;
    let send_timestamp = ntp_timestamp(sent_at)?;
    let mut response = [0u8; TIMING_PACKET_SIZE];
    response[0] = request[0];
    response[1] = TIMING_RESPONSE_TYPE;
    response[2..4].copy_from_slice(&request[2..4]);
    // Bytes 4..8 are the protocol's zero-valued dummy field.
    response[8..16].copy_from_slice(&request[24..32]);
    response[16..24].copy_from_slice(&receive_timestamp);
    response[24..32].copy_from_slice(&send_timestamp);
    Some(response)
}

fn ntp_timestamp(time: SystemTime) -> Option<[u8; 8]> {
    let unix = time.duration_since(UNIX_EPOCH).ok()?;
    let seconds = unix.as_secs().checked_add(NTP_UNIX_EPOCH_OFFSET)? as u32;
    let fraction = ((u64::from(unix.subsec_nanos()) << 32) / 1_000_000_000) as u32;
    let mut timestamp = [0u8; 8];
    timestamp[..4].copy_from_slice(&seconds.to_be_bytes());
    timestamp[4..].copy_from_slice(&fraction.to_be_bytes());
    Some(timestamp)
}

#[cfg(test)]
mod tests {
    use super::{build_timing_response, ntp_timestamp};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn builds_raop_timing_reply() {
        let mut request = [0u8; 32];
        request[0] = 0x80;
        request[1] = 0xd2;
        request[2..4].copy_from_slice(&7u16.to_be_bytes());
        request[24..32].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

        let received_at = UNIX_EPOCH + Duration::from_secs(1);
        let sent_at = UNIX_EPOCH + Duration::from_secs(2);
        let response = build_timing_response(&request, received_at, sent_at).unwrap();

        assert_eq!(&response[..4], &[0x80, 0xd3, 0, 7]);
        assert_eq!(&response[4..8], &[0; 4]);
        assert_eq!(&response[8..16], &request[24..32]);
        assert_eq!(&response[16..24], &ntp_timestamp(received_at).unwrap());
        assert_eq!(&response[24..32], &ntp_timestamp(sent_at).unwrap());
    }

    #[test]
    fn rejects_short_timing_packets() {
        assert!(build_timing_response(&[0u8; 31], UNIX_EPOCH, UNIX_EPOCH).is_none());
    }
}
