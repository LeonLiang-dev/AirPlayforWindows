//! Windows WASAPI loopback audio capture.
//!
//! COM and all WASAPI interfaces live on a dedicated OS thread. The Windows
//! audio engine converts the selected endpoint's mix format to the exact PCM
//! format expected by the AirPlay encoder.
use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};

pub const CAPTURE_SAMPLE_RATE: u32 = 44_100;
pub const CAPTURE_CHANNELS: u16 = 2;
pub const VIRTUAL_ENDPOINT_NAME: &str = "AirPlay Flow Win";
const CAPTURE_CHANNEL_CAPACITY: usize = 64;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Information about an audio output device
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_airplay_flow_virtual: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Enumerate available audio output devices
pub fn enumerate_output_devices() -> AppResult<Vec<AudioDeviceInfo>> {
    platform::enumerate_output_devices()
}

/// Messages produced by the real-time capture thread.
pub enum CaptureMessage {
    Samples(Vec<i16>),
    DeviceChanged(AudioDeviceInfo),
    Error(String),
}

/// Audio capture controller
pub struct AudioCapture {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioCapture {
    /// Create a new audio capture instance
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    /// Start capturing the Windows default output endpoint.
    ///
    /// Startup is acknowledged by the capture thread so callers never report
    /// "Streaming" when WASAPI failed to initialize.
    pub fn start(&mut self) -> AppResult<mpsc::Receiver<CaptureMessage>> {
        if self.running.load(Ordering::SeqCst) {
            return Err(AppError::CaptureError(
                "Audio capture is already running".to_string(),
            ));
        }

        info!("Starting WASAPI loopback capture...");
        self.running.store(true, Ordering::SeqCst);

        let (audio_tx, audio_rx) = mpsc::channel(CAPTURE_CHANNEL_CAPACITY);
        let (startup_tx, startup_rx) = std::sync::mpsc::sync_channel(1);
        let running = self.running.clone();
        let thread = std::thread::Builder::new()
            .name("wasapi-loopback".to_string())
            .spawn(move || platform::run_capture(running, audio_tx, startup_tx))
            .map_err(|error| {
                self.running.store(false, Ordering::SeqCst);
                AppError::CaptureError(format!("Unable to start capture thread: {error}"))
            })?;
        self.thread = Some(thread);

        match startup_rx.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(())) => Ok(audio_rx),
            Ok(Err(message)) => {
                self.stop();
                Err(AppError::CaptureError(message))
            }
            Err(error) => {
                self.stop();
                Err(AppError::CaptureError(format!(
                    "WASAPI capture did not start in time: {error}"
                )))
            }
        }
    }

    /// Stop capturing
    pub fn stop(&mut self) {
        let was_running = self.running.swap(false, Ordering::SeqCst);
        let had_thread = self.thread.is_some();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                warn!("WASAPI capture thread panicked while stopping");
            }
        }
        if was_running || had_thread {
            info!("WASAPI capture stopped");
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::time::Instant;
    use windows::core::{GUID, PWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE, WAVEFORMATEX,
        WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(5);
    const DEFAULT_DEVICE_CHECK_INTERVAL: Duration = Duration::from_millis(500);
    const RESTART_DELAY: Duration = Duration::from_millis(250);
    const BUFFER_DURATION_100NS: i64 = 1_000_000; // 100 ms
    const WAVE_FORMAT_PCM: u16 = 0x0001;
    const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
    const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
    const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
    const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
        GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    pub fn enumerate_output_devices() -> AppResult<Vec<AudioDeviceInfo>> {
        let _com = ComApartment::initialize()?;
        let enumerator = create_enumerator()?;
        let default_id = default_device(&enumerator)
            .and_then(|device| device_id(&device))
            .unwrap_or_default();
        let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
            .map_err(capture_error("Unable to enumerate Windows output devices"))?;
        let count = unsafe { collection.GetCount() }
            .map_err(capture_error("Unable to count Windows output devices"))?;
        let mut devices = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = unsafe { collection.Item(index) }
                .map_err(capture_error("Unable to read a Windows output device"))?;
            let id = device_id(&device)?;
            let name = friendly_name(&device).unwrap_or_else(|_| id.clone());
            let (sample_rate, channels) = mix_format(&device).unwrap_or((0, 0));
            devices.push(AudioDeviceInfo {
                is_default: id == default_id,
                is_airplay_flow_virtual: is_airplay_flow_virtual_endpoint(&name),
                id,
                name,
                sample_rate,
                channels,
            });
        }

        devices.sort_by(|left, right| {
            right
                .is_default
                .cmp(&left.is_default)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(devices)
    }

    pub fn run_capture(
        running: Arc<AtomicBool>,
        audio_tx: mpsc::Sender<CaptureMessage>,
        startup_tx: std::sync::mpsc::SyncSender<Result<(), String>>,
    ) {
        let mut startup_tx = Some(startup_tx);
        let result = capture_main(&running, &audio_tx, &mut startup_tx);

        if let Err(error) = result {
            let message = error.to_string();
            if let Some(startup_tx) = startup_tx.take() {
                let _ = startup_tx.send(Err(message));
            } else if running.load(Ordering::SeqCst) {
                let _ = audio_tx.blocking_send(CaptureMessage::Error(message));
            }
        }
        running.store(false, Ordering::SeqCst);
        info!("Audio capture thread stopped");
    }

    fn capture_main(
        running: &AtomicBool,
        audio_tx: &mpsc::Sender<CaptureMessage>,
        startup_tx: &mut Option<std::sync::mpsc::SyncSender<Result<(), String>>>,
    ) -> AppResult<()> {
        let _com = ComApartment::initialize()?;
        let enumerator = create_enumerator()?;

        while running.load(Ordering::SeqCst) {
            let active_endpoint = default_device(&enumerator)?;
            let active_id = device_id(&active_endpoint)?;
            let active_name = friendly_name(&active_endpoint).unwrap_or_else(|_| active_id.clone());
            let (sample_rate, channels) = mix_format(&active_endpoint).unwrap_or((0, 0));
            let is_airplay_flow_virtual = is_airplay_flow_virtual_endpoint(&active_name);
            let active_device = AudioDeviceInfo {
                id: active_id,
                name: active_name,
                is_default: true,
                is_airplay_flow_virtual,
                sample_rate,
                channels,
            };

            if !active_device.is_airplay_flow_virtual {
                info!(
                    "Windows default output is '{}'; AirPlay capture is paused",
                    active_device.name
                );
                if let Some(startup_tx) = startup_tx.take() {
                    let _ = startup_tx.send(Ok(()));
                }
                if audio_tx
                    .blocking_send(CaptureMessage::DeviceChanged(active_device.clone()))
                    .is_err()
                {
                    return Ok(());
                }

                match wait_for_default_device_change(running, &enumerator, &active_device.id)? {
                    CaptureExit::Stopped => return Ok(()),
                    CaptureExit::DefaultDeviceChanged => {
                        info!("Windows default output changed; checking AirPlay capture state");
                        continue;
                    }
                }
            }

            match WasapiSession::open(&enumerator) {
                Ok(mut session) => {
                    info!(
                        "Capturing '{}' as {} Hz, {} channel PCM",
                        session.device.name, CAPTURE_SAMPLE_RATE, CAPTURE_CHANNELS
                    );
                    if let Some(startup_tx) = startup_tx.take() {
                        let _ = startup_tx.send(Ok(()));
                    }
                    if audio_tx
                        .blocking_send(CaptureMessage::DeviceChanged(session.device.clone()))
                        .is_err()
                    {
                        return Ok(());
                    }

                    match session.capture_until_device_changes(running, audio_tx, &enumerator) {
                        Ok(CaptureExit::Stopped) => return Ok(()),
                        Ok(CaptureExit::DefaultDeviceChanged) => {
                            info!("Windows default output changed; restarting loopback capture");
                        }
                        Err(error) => {
                            warn!("WASAPI capture interrupted: {error}");
                            if audio_tx
                                .blocking_send(CaptureMessage::Error(error.to_string()))
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                }
                Err(error) => {
                    if startup_tx.is_some() {
                        return Err(error);
                    }
                    warn!("Unable to reopen Windows default output: {error}");
                    if audio_tx
                        .blocking_send(CaptureMessage::Error(error.to_string()))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            }

            if !sleep_while_running(running, RESTART_DELAY) {
                return Ok(());
            }
        }

        Ok(())
    }

    struct WasapiSession {
        audio_client: IAudioClient,
        capture_client: IAudioCaptureClient,
        endpoint_volume: IAudioEndpointVolume,
        device: AudioDeviceInfo,
        native_format: NativeAudioFormat,
        converter: AudioConverter,
    }

    impl WasapiSession {
        fn open(enumerator: &IMMDeviceEnumerator) -> AppResult<Self> {
            let endpoint = default_device(enumerator)?;
            let id = device_id(&endpoint)?;
            let name = friendly_name(&endpoint).unwrap_or_else(|_| id.clone());
            let is_airplay_flow_virtual = is_airplay_flow_virtual_endpoint(&name);
            let endpoint_volume: IAudioEndpointVolume =
                unsafe { endpoint.Activate(CLSCTX_ALL, None) }
                    .map_err(capture_error("Unable to access Windows endpoint volume"))?;
            let audio_client: IAudioClient = unsafe { endpoint.Activate(CLSCTX_ALL, None) }
                .map_err(capture_error("Unable to activate the Windows audio client"))?;
            // Microsoft specifies that loopback data uses the endpoint mix
            // format. Capture that exact format and convert in-process; asking
            // the loopback client to auto-convert can initialize successfully
            // on some USB endpoints while never producing capture packets.
            let format_ptr = unsafe { audio_client.GetMixFormat() }
                .map_err(capture_error("Unable to read the WASAPI mix format"))?;
            let native_format = unsafe { NativeAudioFormat::from_wave_format(format_ptr) }?;
            let initialize_result = unsafe {
                audio_client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    BUFFER_DURATION_100NS,
                    0,
                    format_ptr,
                    None,
                )
            };
            unsafe { CoTaskMemFree(Some(format_ptr.cast::<c_void>())) };
            initialize_result.map_err(capture_error(
                "Unable to initialize WASAPI loopback in the device mix format",
            ))?;
            let capture_client = unsafe { audio_client.GetService::<IAudioCaptureClient>() }
                .map_err(capture_error("Unable to create the WASAPI capture client"))?;
            unsafe { audio_client.Start() }
                .map_err(capture_error("Unable to start WASAPI loopback"))?;

            info!(
                "WASAPI native format: {} Hz, {} channels, {}-bit {} (block align {})",
                native_format.sample_rate,
                native_format.channels,
                native_format.bits_per_sample,
                native_format.encoding.name(),
                native_format.block_align
            );
            let converter = AudioConverter::new(native_format);

            Ok(Self {
                audio_client,
                capture_client,
                endpoint_volume,
                device: AudioDeviceInfo {
                    id,
                    name,
                    is_default: true,
                    is_airplay_flow_virtual,
                    sample_rate: native_format.sample_rate,
                    channels: native_format.channels,
                },
                native_format,
                converter,
            })
        }

        fn capture_until_device_changes(
            &mut self,
            running: &AtomicBool,
            audio_tx: &mpsc::Sender<CaptureMessage>,
            enumerator: &IMMDeviceEnumerator,
        ) -> AppResult<CaptureExit> {
            let mut last_device_check = Instant::now();

            while running.load(Ordering::SeqCst) {
                loop {
                    let packet_frames = unsafe { self.capture_client.GetNextPacketSize() }
                        .map_err(capture_error("Unable to query the WASAPI capture buffer"))?;
                    if packet_frames == 0 {
                        break;
                    }

                    let mut data = std::ptr::null_mut();
                    let mut frame_count = 0u32;
                    let mut flags = 0u32;
                    unsafe {
                        self.capture_client.GetBuffer(
                            &mut data,
                            &mut frame_count,
                            &mut flags,
                            None,
                            None,
                        )
                    }
                    .map_err(capture_error("Unable to read the WASAPI capture buffer"))?;

                    let samples = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                        self.converter.convert_silence(frame_count as usize)
                    } else {
                        let byte_count =
                            frame_count as usize * usize::from(self.native_format.block_align);
                        let bytes = unsafe { std::slice::from_raw_parts(data, byte_count) };
                        self.converter.convert(bytes, frame_count as usize)
                    };
                    let mut samples = samples;
                    let gain = self.endpoint_gain()?;
                    apply_gain(&mut samples, gain);
                    let release_result = unsafe { self.capture_client.ReleaseBuffer(frame_count) }
                        .map_err(capture_error("Unable to release the WASAPI capture buffer"));
                    release_result?;

                    if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                        warn!("Windows reported a discontinuity in captured audio");
                    }
                    if !samples.is_empty()
                        && audio_tx
                            .blocking_send(CaptureMessage::Samples(samples))
                            .is_err()
                    {
                        return Ok(CaptureExit::Stopped);
                    }
                }

                if last_device_check.elapsed() >= DEFAULT_DEVICE_CHECK_INTERVAL {
                    let active_id =
                        default_device(enumerator).and_then(|device| device_id(&device))?;
                    if active_id != self.device.id {
                        return Ok(CaptureExit::DefaultDeviceChanged);
                    }
                    last_device_check = Instant::now();
                }

                std::thread::sleep(POLL_INTERVAL);
            }

            Ok(CaptureExit::Stopped)
        }

        fn endpoint_gain(&self) -> AppResult<f32> {
            let muted = unsafe { self.endpoint_volume.GetMute() }
                .map_err(capture_error("Unable to read Windows endpoint mute state"))?;
            if muted.as_bool() {
                return Ok(0.0);
            }

            let level_db = unsafe { self.endpoint_volume.GetMasterVolumeLevel() }
                .map_err(capture_error("Unable to read Windows endpoint volume"))?;
            Ok(decibels_to_gain(level_db))
        }
    }

    impl Drop for WasapiSession {
        fn drop(&mut self) {
            if let Err(error) = unsafe { self.audio_client.Stop() } {
                warn!("Unable to stop WASAPI audio client cleanly: {error}");
            }
        }
    }

    enum CaptureExit {
        Stopped,
        DefaultDeviceChanged,
    }

    struct ComApartment {
        uninitialize: bool,
    }

    impl ComApartment {
        fn initialize() -> AppResult<Self> {
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if result == RPC_E_CHANGED_MODE {
                return Ok(Self {
                    uninitialize: false,
                });
            }
            result
                .ok()
                .map_err(capture_error("Unable to initialize COM for Windows audio"))?;
            Ok(Self { uninitialize: true })
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.uninitialize {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn create_enumerator() -> AppResult<IMMDeviceEnumerator> {
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(capture_error("Unable to access Windows audio devices"))
    }

    fn default_device(enumerator: &IMMDeviceEnumerator) -> AppResult<IMMDevice> {
        unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
            .map_err(capture_error("Windows has no active default output device"))
    }

    fn device_id(device: &IMMDevice) -> AppResult<String> {
        let value = unsafe { device.GetId() }
            .map_err(capture_error("Unable to read the Windows audio device ID"))?;
        pwstr_to_string_and_free(value, "Windows audio device ID is invalid")
    }

    fn friendly_name(device: &IMMDevice) -> AppResult<String> {
        let store = unsafe { device.OpenPropertyStore(STGM_READ) }.map_err(capture_error(
            "Unable to open Windows audio device properties",
        ))?;
        let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }
            .map_err(capture_error("Unable to read Windows audio device name"))?;
        let text = unsafe { PropVariantToStringAlloc(&value) }
            .map_err(capture_error("Unable to convert Windows audio device name"))?;
        pwstr_to_string_and_free(text, "Windows audio device name is invalid")
    }

    fn mix_format(device: &IMMDevice) -> AppResult<(u32, u16)> {
        let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(
            capture_error("Unable to inspect Windows audio device format"),
        )?;
        let format_ptr = unsafe { audio_client.GetMixFormat() }
            .map_err(capture_error("Unable to read Windows audio device format"))?;
        let format = unsafe { format_ptr.read_unaligned() };
        unsafe { CoTaskMemFree(Some(format_ptr.cast::<c_void>())) };
        Ok((format.nSamplesPerSec, format.nChannels))
    }

    #[derive(Clone, Copy)]
    enum SampleEncoding {
        Pcm,
        Float,
    }

    impl SampleEncoding {
        fn name(self) -> &'static str {
            match self {
                Self::Pcm => "PCM",
                Self::Float => "float",
            }
        }
    }

    #[derive(Clone, Copy)]
    struct NativeAudioFormat {
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
        valid_bits_per_sample: u16,
        block_align: u16,
        encoding: SampleEncoding,
    }

    impl NativeAudioFormat {
        unsafe fn from_wave_format(format_ptr: *const WAVEFORMATEX) -> AppResult<Self> {
            let format = unsafe { format_ptr.read_unaligned() };
            let format_tag = format.wFormatTag;
            let bits_per_sample = format.wBitsPerSample;
            let sample_rate = format.nSamplesPerSec;
            let channels = format.nChannels;
            let block_align = format.nBlockAlign;
            let (encoding, valid_bits_per_sample) = match format_tag {
                WAVE_FORMAT_PCM => (SampleEncoding::Pcm, bits_per_sample),
                WAVE_FORMAT_IEEE_FLOAT => (SampleEncoding::Float, bits_per_sample),
                WAVE_FORMAT_EXTENSIBLE if format.cbSize >= 22 => {
                    let extensible =
                        unsafe { format_ptr.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
                    let sub_format = extensible.SubFormat;
                    let valid_bits = unsafe { extensible.Samples.wValidBitsPerSample };
                    let encoding = if sub_format == KSDATAFORMAT_SUBTYPE_PCM {
                        SampleEncoding::Pcm
                    } else if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                        SampleEncoding::Float
                    } else {
                        return Err(AppError::CaptureError(format!(
                            "Unsupported WASAPI extensible sub-format: {sub_format:?}"
                        )));
                    };
                    (encoding, valid_bits.max(1).min(bits_per_sample))
                }
                _ => {
                    return Err(AppError::CaptureError(format!(
                        "Unsupported WASAPI mix format tag: 0x{format_tag:04x}"
                    )))
                }
            };

            let supported = match encoding {
                SampleEncoding::Pcm => matches!(bits_per_sample, 8 | 16 | 24 | 32),
                SampleEncoding::Float => matches!(bits_per_sample, 32 | 64),
            };
            if !supported || channels == 0 || sample_rate == 0 {
                return Err(AppError::CaptureError(format!(
                    "Unsupported WASAPI format: {} Hz, {} channels, {}-bit {}",
                    sample_rate,
                    channels,
                    bits_per_sample,
                    encoding.name()
                )));
            }

            Ok(Self {
                sample_rate,
                channels,
                bits_per_sample,
                valid_bits_per_sample,
                block_align,
                encoding,
            })
        }

        fn bytes_per_sample(self) -> usize {
            usize::from(self.bits_per_sample / 8)
        }

        fn decode_sample(self, bytes: &[u8]) -> f32 {
            let sample = match (self.encoding, self.bits_per_sample) {
                (SampleEncoding::Pcm, 8) => (f32::from(bytes[0]) - 128.0) / 128.0,
                (SampleEncoding::Pcm, 16) => {
                    f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0
                }
                (SampleEncoding::Pcm, 24) => {
                    let raw = i32::from_le_bytes([
                        bytes[0],
                        bytes[1],
                        bytes[2],
                        if bytes[2] & 0x80 != 0 { 0xff } else { 0 },
                    ]);
                    raw as f32 / 8_388_608.0
                }
                (SampleEncoding::Pcm, 32) => {
                    let raw = i32::from_le_bytes(bytes[..4].try_into().unwrap_or_default());
                    let shift = 32u16.saturating_sub(self.valid_bits_per_sample);
                    let value = raw >> shift;
                    let scale = (1u64 << (self.valid_bits_per_sample - 1)) as f32;
                    value as f32 / scale
                }
                (SampleEncoding::Float, 32) => {
                    f32::from_le_bytes(bytes[..4].try_into().unwrap_or_default())
                }
                (SampleEncoding::Float, 64) => {
                    f64::from_le_bytes(bytes[..8].try_into().unwrap_or_default()) as f32
                }
                _ => 0.0,
            };
            if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        }
    }

    struct AudioConverter {
        format: NativeAudioFormat,
        resampler: StereoResampler,
    }

    impl AudioConverter {
        fn new(format: NativeAudioFormat) -> Self {
            Self {
                resampler: StereoResampler::new(format.sample_rate, CAPTURE_SAMPLE_RATE),
                format,
            }
        }

        fn convert(&mut self, bytes: &[u8], frame_count: usize) -> Vec<i16> {
            let block_align = usize::from(self.format.block_align);
            let bytes_per_sample = self.format.bytes_per_sample();
            let available_frames = frame_count.min(bytes.len() / block_align);
            let mut stereo = Vec::with_capacity(available_frames);
            for frame_index in 0..available_frames {
                let frame = &bytes[frame_index * block_align..(frame_index + 1) * block_align];
                let left = self.format.decode_sample(&frame[..bytes_per_sample]);
                let right = if self.format.channels > 1 {
                    self.format
                        .decode_sample(&frame[bytes_per_sample..bytes_per_sample * 2])
                } else {
                    left
                };
                stereo.push([left, right]);
            }
            self.resampler.push(&stereo)
        }

        fn convert_silence(&mut self, frame_count: usize) -> Vec<i16> {
            self.resampler.push(&vec![[0.0, 0.0]; frame_count])
        }
    }

    struct StereoResampler {
        step: f64,
        position: f64,
        buffered: Vec<[f32; 2]>,
    }

    impl StereoResampler {
        fn new(input_rate: u32, output_rate: u32) -> Self {
            Self {
                step: input_rate as f64 / output_rate as f64,
                position: 0.0,
                buffered: Vec::new(),
            }
        }

        fn push(&mut self, frames: &[[f32; 2]]) -> Vec<i16> {
            self.buffered.extend_from_slice(frames);
            let estimated_frames = (frames.len() as f64 / self.step).ceil().max(0.0) as usize;
            let mut output = Vec::with_capacity(estimated_frames * 2);

            while self.position + 1.0 < self.buffered.len() as f64 {
                let index = self.position.floor() as usize;
                let fraction = (self.position - index as f64) as f32;
                let current = self.buffered[index];
                let next = self.buffered[index + 1];
                for channel in 0..2 {
                    let sample = current[channel] + (next[channel] - current[channel]) * fraction;
                    output.push(float_to_i16(sample));
                }
                self.position += self.step;
            }

            let consumed = (self.position.floor() as usize).min(self.buffered.len());
            if consumed > 0 {
                self.buffered.drain(..consumed);
                self.position -= consumed as f64;
            }
            output
        }
    }

    fn float_to_i16(sample: f32) -> i16 {
        (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16
    }

    fn decibels_to_gain(level_db: f32) -> f32 {
        if level_db.is_finite() {
            10.0f32.powf(level_db / 20.0).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn apply_gain(samples: &mut [i16], gain: f32) {
        if gain <= f32::EPSILON {
            samples.fill(0);
            return;
        }
        if (gain - 1.0).abs() <= f32::EPSILON {
            return;
        }
        for sample in samples {
            *sample = (f32::from(*sample) * gain)
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        }
    }

    fn pwstr_to_string_and_free(value: PWSTR, context: &'static str) -> AppResult<String> {
        let result = unsafe { value.to_string() };
        unsafe { CoTaskMemFree(Some(value.0.cast::<c_void>())) };
        result.map_err(|error| AppError::CaptureError(format!("{context}: {error}")))
    }

    fn capture_error(context: &'static str) -> impl FnOnce(windows::core::Error) -> AppError {
        move |error| AppError::CaptureError(format!("{context}: {error}"))
    }

    fn is_airplay_flow_virtual_endpoint(name: &str) -> bool {
        name.to_ascii_lowercase()
            .contains(&VIRTUAL_ENDPOINT_NAME.to_ascii_lowercase())
    }

    fn sleep_while_running(running: &AtomicBool, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        while running.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(POLL_INTERVAL);
        }
        running.load(Ordering::SeqCst)
    }

    fn wait_for_default_device_change(
        running: &AtomicBool,
        enumerator: &IMMDeviceEnumerator,
        current_id: &str,
    ) -> AppResult<CaptureExit> {
        while sleep_while_running(running, DEFAULT_DEVICE_CHECK_INTERVAL) {
            let active_id = default_device(enumerator).and_then(|device| device_id(&device))?;
            if active_id != current_id {
                return Ok(CaptureExit::DefaultDeviceChanged);
            }
        }
        Ok(CaptureExit::Stopped)
    }

    #[cfg(test)]
    mod conversion_tests {
        use super::{
            apply_gain, decibels_to_gain, is_airplay_flow_virtual_endpoint, AudioConverter,
            NativeAudioFormat, SampleEncoding, StereoResampler,
        };

        #[test]
        fn recognizes_the_virtual_render_endpoint_name() {
            assert!(is_airplay_flow_virtual_endpoint(
                "Speakers (AirPlay Flow Win Virtual Audio)"
            ));
            assert!(!is_airplay_flow_virtual_endpoint("Headphones (USB Audio)"));
        }

        #[test]
        fn converts_native_float_stereo_to_interleaved_i16() {
            let format = NativeAudioFormat {
                sample_rate: 44_100,
                channels: 2,
                bits_per_sample: 32,
                valid_bits_per_sample: 32,
                block_align: 8,
                encoding: SampleEncoding::Float,
            };
            let mut bytes = Vec::new();
            for frame in [[0.5f32, -0.5f32], [1.0, -1.0], [0.0, 0.0]] {
                bytes.extend_from_slice(&frame[0].to_le_bytes());
                bytes.extend_from_slice(&frame[1].to_le_bytes());
            }

            let converted = AudioConverter::new(format).convert(&bytes, 3);

            assert_eq!(converted, vec![16_384, -16_384, 32_767, -32_767]);
        }

        #[test]
        fn resamples_48khz_stereo_to_44_1khz() {
            let mut resampler = StereoResampler::new(48_000, 44_100);
            let input = vec![[0.25, -0.25]; 480];

            let converted = resampler.push(&input);

            assert_eq!(converted.len(), 441 * 2);
            assert!(converted
                .chunks_exact(2)
                .all(|frame| frame == [8_192, -8_192]));
        }

        #[test]
        fn converts_decibels_to_pcm_gain() {
            assert!((decibels_to_gain(0.0) - 1.0).abs() < 0.000_001);
            assert!((decibels_to_gain(-6.020_6) - 0.5).abs() < 0.000_1);
            assert_eq!(decibels_to_gain(f32::NAN), 0.0);
        }

        #[test]
        fn applies_endpoint_gain_and_mute_to_pcm() {
            let mut samples = [20_000, -20_000, 1, -1];
            apply_gain(&mut samples, 0.5);
            assert_eq!(samples, [10_000, -10_000, 1, -1]);

            apply_gain(&mut samples, 0.0);
            assert_eq!(samples, [0; 4]);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn enumerate_output_devices() -> AppResult<Vec<AudioDeviceInfo>> {
        Err(AppError::CaptureError(
            "WASAPI loopback is only available on Windows".to_string(),
        ))
    }

    pub fn run_capture(
        _running: Arc<AtomicBool>,
        _audio_tx: mpsc::Sender<CaptureMessage>,
        startup_tx: std::sync::mpsc::SyncSender<Result<(), String>>,
    ) {
        let _ = startup_tx.send(Err(
            "WASAPI loopback is only available on Windows".to_string()
        ));
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{enumerate_output_devices, AudioCapture, CaptureMessage};
    use std::time::Duration;

    #[test]
    #[ignore = "requires an active Windows audio output device"]
    fn enumerates_and_starts_default_loopback_device() {
        let devices = enumerate_output_devices().expect("enumerate Windows output devices");
        assert!(devices.iter().any(|device| device.is_default));

        let mut capture = AudioCapture::new();
        let mut receiver = capture.start().expect("start WASAPI loopback");
        let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
        let message = runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(2), receiver.recv()).await })
            .expect("receive capture startup event")
            .expect("capture channel remains open");
        assert!(matches!(message, CaptureMessage::DeviceChanged(_)));
        capture.stop();
    }
}
