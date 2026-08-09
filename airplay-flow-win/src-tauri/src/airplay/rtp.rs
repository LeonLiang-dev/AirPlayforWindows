/// RTP packet construction and UDP streaming
///
/// Handles RTP header construction, ALAC payload framing, and UDP socket management.
use log::debug;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

use crate::error::{AppError, AppResult};

/// RTP header size (fixed 12 bytes)
const RTP_HEADER_SIZE: usize = 12;

/// RTP version 2
const RTP_VERSION: u8 = 2;

const NTP_UNIX_EPOCH_OFFSET: u64 = 2_208_988_800;

/// RAOP synchronization packets use a fixed RTP payload type and sequence.
const SYNC_PAYLOAD_TYPE: u8 = 0x54;
const SYNC_SEQUENCE: u16 = 7;

/// An RTP stream to a single AirPlay device
pub struct RtpStream {
    socket: UdpSocket,
    remote_addr: SocketAddr,
    sequence_number: u16,
    /// Full-width media timestamp. Audio packets carry its low 32 bits while
    /// synchronization packets also use it to reconstruct the NTP clock.
    rtp_timestamp: u64,
    ssrc: u32,
    payload_type: u8,
    sample_rate: u32,
    first_packet: bool,
}

impl RtpStream {
    /// Create a new RTP stream bound to a local UDP port
    pub async fn new(
        remote_addr: SocketAddr,
        payload_type: u8,
        sample_rate: u32,
    ) -> AppResult<Self> {
        // Bind to an ephemeral port
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| AppError::StreamError(format!("Failed to bind UDP socket: {}", e)))?;

        socket
            .connect(remote_addr)
            .await
            .map_err(|e| AppError::StreamError(format!("Failed to connect UDP socket: {}", e)))?;

        let ssrc = rand::random::<u32>();

        debug!(
            "RTP stream created: local={}, remote={}, ssrc={}",
            socket
                .local_addr()
                .unwrap_or_else(|_| std::net::SocketAddr::from(([0, 0, 0, 0], 0))),
            remote_addr,
            ssrc
        );

        Ok(Self {
            socket,
            remote_addr,
            sequence_number: rand::random::<u16>(),
            rtp_timestamp: current_media_timestamp(sample_rate),
            ssrc,
            payload_type,
            sample_rate,
            first_packet: true,
        })
    }

    /// Send an ALAC-encoded audio frame via RTP/UDP
    pub async fn send_frame(&mut self, alac_data: &[u8], samples_in_frame: u32) -> AppResult<()> {
        let packet = build_rtp_packet(
            self.payload_type,
            self.sequence_number,
            self.timestamp(),
            self.ssrc,
            self.first_packet,
            alac_data,
        );

        self.socket
            .send(&packet)
            .await
            .map_err(|e| AppError::StreamError(format!("UDP send failed: {}", e)))?;

        // Advance RTP sequence and timestamp
        self.sequence_number = self.sequence_number.wrapping_add(1);
        self.rtp_timestamp = self.rtp_timestamp.wrapping_add(u64::from(samples_in_frame));
        self.first_packet = false;

        Ok(())
    }

    /// Send raw RTP packet bytes (for pre-built packets)
    pub async fn send_raw(&mut self, packet: &[u8]) -> AppResult<()> {
        self.socket
            .send(packet)
            .await
            .map_err(|e| AppError::StreamError(format!("UDP send failed: {}", e)))?;

        self.sequence_number = self.sequence_number.wrapping_add(1);
        Ok(())
    }

    /// Rebase the media clock immediately before RECORD.
    pub fn prepare_recording(&mut self) {
        self.rtp_timestamp = current_media_timestamp(self.sample_rate);
        self.first_packet = true;
    }

    /// Reinitialize sequence numbers and media clock (used after FLUSH).
    pub fn reset_sequence(&mut self) {
        self.sequence_number = rand::random::<u16>();
        self.prepare_recording();
    }

    /// Get the current RTP timestamp
    pub fn timestamp(&self) -> u32 {
        self.rtp_timestamp as u32
    }

    /// Get the full media timestamp used to construct RAOP sync packets.
    pub fn media_timestamp(&self) -> u64 {
        self.rtp_timestamp
    }

    /// Get the sequence number that will be used for the next packet.
    pub fn sequence_number(&self) -> u16 {
        self.sequence_number
    }

    /// Get the remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
}

/// Build an RTP packet containing an ALAC audio frame.
fn build_rtp_packet(
    payload_type: u8,
    sequence_number: u16,
    rtp_timestamp: u32,
    ssrc: u32,
    marker: bool,
    payload: &[u8],
) -> Vec<u8> {
    let packet_size = RTP_HEADER_SIZE + payload.len();
    let mut packet = vec![0u8; packet_size];

    // RTP Header (RFC 3550): V=2, P=0, X=0, CC=0.
    packet[0] = RTP_VERSION << 6;
    // RAOP receivers use the marker bit to identify the first audio packet.
    packet[1] = (payload_type & 0x7F) | if marker { 0x80 } else { 0 };
    packet[2..4].copy_from_slice(&sequence_number.to_be_bytes());
    packet[4..8].copy_from_slice(&rtp_timestamp.to_be_bytes());
    packet[8..12].copy_from_slice(&ssrc.to_be_bytes());
    packet[RTP_HEADER_SIZE..].copy_from_slice(payload);
    packet
}

/// Build the 20-byte RAOP control synchronization packet.
pub(crate) fn build_sync_packet(
    media_timestamp: u64,
    sample_rate: u32,
    latency_frames: u32,
    first: bool,
) -> [u8; 20] {
    let mut packet = [0u8; 20];
    // The first sync packet sets the RTP extension bit, as expected by RAOP v2.
    packet[0] = (RTP_VERSION << 6) | if first { 0x10 } else { 0 };
    packet[1] = SYNC_PAYLOAD_TYPE | 0x80;
    packet[2..4].copy_from_slice(&SYNC_SEQUENCE.to_be_bytes());
    packet[4..8].copy_from_slice(
        &(media_timestamp.wrapping_sub(u64::from(latency_frames)) as u32).to_be_bytes(),
    );
    packet[8..16].copy_from_slice(&ntp_from_media_timestamp(media_timestamp, sample_rate));
    packet[16..20].copy_from_slice(&(media_timestamp as u32).to_be_bytes());
    packet
}

fn current_media_timestamp(sample_rate: u32) -> u64 {
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ntp_seconds = unix.as_secs().saturating_add(NTP_UNIX_EPOCH_OFFSET);
    let ntp_fraction = ((u64::from(unix.subsec_nanos()) << 32) / 1_000_000_000) as u32;
    let ntp = (u128::from(ntp_seconds) << 32) | u128::from(ntp_fraction);
    (((ntp >> 16) * u128::from(sample_rate)) >> 16) as u64
}

fn ntp_from_media_timestamp(media_timestamp: u64, sample_rate: u32) -> [u8; 8] {
    let ntp = ((u128::from(media_timestamp) << 16) / u128::from(sample_rate)) << 16;
    (ntp as u64).to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::{build_rtp_packet, build_sync_packet};

    #[test]
    fn marks_only_the_first_raop_audio_packet() {
        let first = build_rtp_packet(96, 0x1234, 0x1020_3040, 0x5060_7080, true, &[1, 2]);
        let following = build_rtp_packet(96, 0x1235, 0x1020_31a0, 0x5060_7080, false, &[3]);

        assert_eq!(&first[..4], &[0x80, 0xe0, 0x12, 0x34]);
        assert_eq!(&following[..4], &[0x80, 0x60, 0x12, 0x35]);
        assert_eq!(&first[4..8], &0x1020_3040u32.to_be_bytes());
        assert_eq!(&first[8..12], &0x5060_7080u32.to_be_bytes());
        assert_eq!(&first[12..], &[1, 2]);
    }

    #[test]
    fn builds_initial_and_periodic_raop_sync_packets() {
        let timestamp = u64::from(2_208_988_801u32) * 44_100;
        let first = build_sync_packet(timestamp, 44_100, 11_025, true);
        let periodic = build_sync_packet(timestamp, 44_100, 11_025, false);

        assert_eq!(&first[..4], &[0x90, 0xd4, 0, 7]);
        assert_eq!(&periodic[..4], &[0x80, 0xd4, 0, 7]);
        assert_eq!(&first[4..8], &((timestamp - 11_025) as u32).to_be_bytes());
        let expected_ntp = (2_208_988_801u64 << 32).to_be_bytes();
        assert_eq!(&first[8..16], &expected_ntp);
        assert_eq!(&first[16..20], &(timestamp as u32).to_be_bytes());
    }
}
