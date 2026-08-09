/// Audio pipeline orchestrator.
///
/// Connects audio capture -> ALAC encoding -> RTP distribution to active streams.
use log::{info, warn};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::airplay::session::RtspSession;
use crate::audio::capture::{AudioCapture, CaptureMessage, CAPTURE_CHANNELS, CAPTURE_SAMPLE_RATE};
use crate::audio::encoder::AlacEncoder;
use crate::error::AppResult;

const SAMPLE_RATE: f64 = CAPTURE_SAMPLE_RATE as f64;
const CHANNELS: usize = CAPTURE_CHANNELS as usize;
const FRAME_SAMPLES: usize = 352;
const MAX_PENDING_SAMPLES: usize = CAPTURE_SAMPLE_RATE as usize * CHANNELS / 2;
const ACTIVITY_LOG_INTERVAL: Duration = Duration::from_secs(2);

pub enum AudioPipelineEvent {
    CaptureDeviceChanged(crate::audio::capture::AudioDeviceInfo),
    CaptureError(String),
}

pub struct AudioPipeline {
    stop_tx: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
    capture: Option<AudioCapture>,
}

impl Default for AudioPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPipeline {
    pub fn new() -> Self {
        Self {
            stop_tx: None,
            worker: None,
            capture: None,
        }
    }

    /// Start WASAPI capture, ALAC encoding, packet scheduling, and distribution.
    pub async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<String, RtspSession>>>,
        device_ids: Vec<String>,
        event_tx: Option<mpsc::UnboundedSender<AudioPipelineEvent>>,
    ) -> AppResult<()> {
        if self.worker.is_some() {
            return Ok(());
        }

        let mut capture = AudioCapture::new();
        let mut capture_rx = capture.start()?;
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);
        self.capture = Some(capture);

        self.worker = Some(tokio::spawn(async move {
            let mut encoder = AlacEncoder::new(SAMPLE_RATE, CHANNELS as u32);
            let mut pending = VecDeque::<i16>::new();
            let frame_period = Duration::from_secs_f64(FRAME_SAMPLES as f64 / SAMPLE_RATE);
            let mut ticker = tokio::time::interval(frame_period);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
            let mut captured_samples = 0usize;
            let mut nonzero_samples = 0usize;
            let mut padded_samples = 0usize;
            let mut peak_sample = 0i32;
            let mut last_activity_log = Instant::now();

            info!("Audio pipeline started for {} device(s)", device_ids.len());

            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    message = capture_rx.recv() => {
                        match message {
                            Some(CaptureMessage::Samples(samples)) => {
                                captured_samples += samples.len();
                                nonzero_samples += samples.iter().filter(|sample| **sample != 0).count();
                                peak_sample = peak_sample.max(
                                    samples
                                        .iter()
                                        .map(|sample| i32::from(*sample).abs())
                                        .max()
                                        .unwrap_or_default(),
                                );
                                pending.extend(samples);
                                if pending.len() > MAX_PENDING_SAMPLES {
                                    let overflow = pending.len() - MAX_PENDING_SAMPLES;
                                    pending.drain(..overflow);
                                    warn!("Audio capture fell behind; dropped {overflow} old samples");
                                }
                            }
                            Some(CaptureMessage::DeviceChanged(device)) => {
                                if !device.is_airplay_flow_virtual {
                                    pending.clear();
                                    info!(
                                        "AirPlay output paused because Windows selected '{}'",
                                        device.name
                                    );
                                }
                                info!("Audio capture source is now '{}'", device.name);
                                if let Some(event_tx) = event_tx.as_ref() {
                                    let _ = event_tx.send(AudioPipelineEvent::CaptureDeviceChanged(device));
                                }
                            }
                            Some(CaptureMessage::Error(error)) => {
                                warn!("Audio capture error: {error}");
                                if let Some(event_tx) = event_tx.as_ref() {
                                    let _ = event_tx.send(AudioPipelineEvent::CaptureError(error));
                                }
                            }
                            None => {
                                warn!("Audio capture ended unexpectedly");
                                break;
                            }
                        }
                    }
                    _ = ticker.tick() => {
                        let required_samples = FRAME_SAMPLES * CHANNELS;
                        let mut pcm_frame = vec![0i16; required_samples];
                        let available = pending.len().min(required_samples);
                        padded_samples += required_samples - available;
                        for sample in pcm_frame.iter_mut().take(available) {
                            *sample = pending.pop_front().unwrap_or_default();
                        }
                        let alac_frame = encoder.encode_frame(&pcm_frame).to_vec();
                        let mut sessions = sessions.lock().await;
                        for device_id in &device_ids {
                            if let Some(session) = sessions.get_mut(device_id) {
                                if let Err(error) = session.send_audio(&alac_frame).await {
                                    warn!("Unable to send audio to {device_id}: {error}");
                                }
                            }
                        }

                        if last_activity_log.elapsed() >= ACTIVITY_LOG_INTERVAL {
                            let peak = peak_sample as f32 / i16::MAX as f32;
                            let queued_ms = pending.len() as f64
                                / (CAPTURE_SAMPLE_RATE as f64 * CHANNELS as f64)
                                * 1_000.0;
                            if nonzero_samples == 0 {
                                warn!(
                                    "Captured PCM is silent: samples={captured_samples}, padded={padded_samples}, queued={queued_ms:.1} ms"
                                );
                            } else {
                                info!(
                                    "Captured PCM is active: peak={peak:.3}, nonzero={nonzero_samples}/{captured_samples}, padded={padded_samples}, queued={queued_ms:.1} ms"
                                );
                            }
                            captured_samples = 0;
                            nonzero_samples = 0;
                            padded_samples = 0;
                            peak_sample = 0;
                            last_activity_log = Instant::now();
                        }
                    }
                }
            }

            info!("Audio pipeline stopped");
        }));

        Ok(())
    }

    pub async fn stop(&mut self) -> AppResult<()> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()).await;
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.await;
        }
        if let Some(mut capture) = self.capture.take() {
            capture.stop();
        }
        Ok(())
    }
}
