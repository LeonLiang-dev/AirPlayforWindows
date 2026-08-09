/// SDP (Session Description Protocol) generation for RAOP ANNOUNCE requests
///
/// Generates the SDP body describing the audio stream parameters.
/// Configuration for the audio stream
pub struct StreamConfig {
    pub sample_rate: u32, // e.g. 44100
    pub channels: u32,    // e.g. 2
    pub frame_size: u32,  // ALAC frames per packet, e.g. 352
    pub payload_type: u8, // RTP payload type, typically 96
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            channels: 2,
            frame_size: 352,
            payload_type: 96,
        }
    }
}

/// ALAC decoder configuration as hex string
fn alac_config_hex(config: &StreamConfig) -> String {
    // ALAC magic cookie / decoder configuration
    // This is a simplified minimal config for 44100/2ch/16bit
    format!(
        "{} 0 16 40 10 14 2 255 0 0 {}",
        config.frame_size, config.sample_rate
    )
}

/// Build the full SDP description for an ANNOUNCE request
pub fn build_sdp(
    config: &StreamConfig,
    local_ip: &str,
    remote_ip: &str,
    daap_session_id: u32,
) -> String {
    let fmtp = alac_config_hex(config);

    format!(
        "v=0\r\n\
         o=iTunes {session_id} 0 IN IP4 {local}\r\n\
         s=iTunes\r\n\
         c=IN IP4 {remote}\r\n\
         t=0 0\r\n\
         m=audio 0 RTP/AVP {pt}\r\n\
         a=rtpmap:{pt} AppleLossless\r\n\
         a=fmtp:{pt} {fmtp}\r\n\
         a=min-latency:11025\r\n",
        session_id = daap_session_id,
        local = local_ip,
        remote = remote_ip,
        pt = config.payload_type,
        fmtp = fmtp,
    )
}

#[cfg(test)]
mod tests {
    use super::{build_sdp, StreamConfig};

    #[test]
    fn builds_a_clear_alac_session_description() {
        let sdp = build_sdp(&StreamConfig::default(), "192.168.1.10", "192.168.1.20", 42);

        assert!(sdp.contains("o=iTunes 42 0 IN IP4 192.168.1.10"));
        assert!(sdp.contains("c=IN IP4 192.168.1.20"));
        assert!(sdp.contains("a=rtpmap:96 AppleLossless"));
        assert!(!sdp.contains("rsaaeskey"));
        assert!(!sdp.contains("aesiv"));
    }
}
