/// PCM to ALAC audio encoder
///
/// Wraps the `alac-encoder` crate for streaming use.
use log::debug;

/// ALAC frame encoder for real-time audio streaming
pub struct AlacEncoder {
    inner: alac_encoder::AlacEncoder,
    input_format: alac_encoder::FormatDescription,
    #[allow(dead_code)]
    output_format: alac_encoder::FormatDescription,
    frame_size: usize,
    channels: u32,
    #[allow(dead_code)]
    sample_rate: f64,
    output_buffer: Vec<u8>,
}

impl AlacEncoder {
    /// Create a new ALAC encoder
    ///
    /// * `sample_rate` - Audio sample rate in Hz (typically 44100.0)
    /// * `channels` - Number of audio channels (typically 2)
    pub fn new(sample_rate: f64, channels: u32) -> Self {
        let frame_size = 352; // Standard ALAC frame size for 44.1kHz

        let input_format = alac_encoder::FormatDescription::pcm::<i16>(sample_rate, channels);
        let output_format =
            alac_encoder::FormatDescription::alac(sample_rate, frame_size, channels);

        let inner = alac_encoder::AlacEncoder::new(&output_format);
        let output_buffer = vec![0u8; output_format.max_packet_size()];

        debug!(
            "ALAC encoder created: {} Hz, {} ch, frame_size={}",
            sample_rate, channels, frame_size
        );

        Self {
            inner,
            input_format,
            output_format,
            frame_size: frame_size as usize,
            channels,
            sample_rate,
            output_buffer,
        }
    }

    /// Encode a single frame of interleaved PCM i16 samples
    ///
    /// `pcm_data` must contain exactly `frame_size * channels` samples.
    /// Returns a slice into the internal output buffer.
    pub fn encode_frame(&mut self, pcm_data: &[i16]) -> &[u8] {
        let expected_samples = self.frame_size * self.channels as usize;
        let input = if pcm_data.len() < expected_samples {
            // Pad with silence if input is shorter than expected
            let mut padded = vec![0i16; expected_samples];
            let copy_len = pcm_data.len().min(expected_samples);
            padded[..copy_len].copy_from_slice(&pcm_data[..copy_len]);
            padded
        } else {
            pcm_data[..expected_samples].to_vec()
        };

        // Convert i16 samples to u8 bytes via pointer cast
        let input_bytes =
            unsafe { std::slice::from_raw_parts(input.as_ptr() as *const u8, input.len() * 2) };
        let size = self
            .inner
            .encode(&self.input_format, input_bytes, &mut self.output_buffer);
        &self.output_buffer[..size]
    }

    /// Encode from f32 samples, converting to i16
    pub fn encode_f32(&mut self, pcm_f32: &[f32]) -> &[u8] {
        let pcm_i16: Vec<i16> = pcm_f32
            .iter()
            .map(|s| (*s as f64 * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect();

        self.encode_frame(&pcm_i16)
    }
}
