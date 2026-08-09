use app_lib::audio::capture::{enumerate_output_devices, AudioCapture, CaptureMessage};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = enumerate_output_devices()?;
    let default = devices
        .iter()
        .find(|device| device.is_default)
        .ok_or("Windows has no default output device")?;
    println!(
        "default={} virtual={} format={}Hz/{}ch",
        default.name, default.is_airplay_flow_virtual, default.sample_rate, default.channels
    );
    let mut capture = AudioCapture::new();
    let mut receiver = capture.start()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut sample_count = 0usize;
    let mut nonzero_count = 0usize;
    let mut clipped_count = 0usize;
    let mut peak = 0i32;
    let mut square_sum = 0f64;
    let mut analysis_left = Vec::with_capacity(44_100);
    let mut source = None;

    runtime.block_on(async {
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = match tokio::time::timeout(remaining, receiver.recv()).await {
                Ok(Some(message)) => message,
                Ok(None) | Err(_) => break,
            };
            match message {
                CaptureMessage::DeviceChanged(device) => source = Some(device.name),
                CaptureMessage::Samples(samples) => {
                    for sample in samples {
                        let magnitude = i32::from(sample).abs();
                        if sample_count % 2 == 0 && analysis_left.len() < 44_100 {
                            analysis_left.push(f64::from(sample) / f64::from(i16::MAX));
                        }
                        sample_count += 1;
                        nonzero_count += usize::from(sample != 0);
                        clipped_count += usize::from(magnitude >= 32_760);
                        peak = peak.max(magnitude);
                        let normalized = f64::from(sample) / f64::from(i16::MAX);
                        square_sum += normalized * normalized;
                    }
                }
                CaptureMessage::Error(error) => {
                    eprintln!("capture-error={error}");
                    break;
                }
            }
        }
    });
    capture.stop();

    let rms = if sample_count == 0 {
        0.0
    } else {
        (square_sum / sample_count as f64).sqrt()
    };
    println!("source={}", source.unwrap_or_else(|| "unknown".to_string()));
    println!(
        "samples={sample_count} nonzero={nonzero_count} peak={:.6} rms={rms:.6} clipped={clipped_count}",
        peak as f64 / f64::from(i16::MAX)
    );
    let tone_3khz = sinusoid_amplitude(&analysis_left, 44_100.0, 3_000.0);
    println!("tone_3khz_amplitude={tone_3khz:.6}");
    Ok(())
}

fn sinusoid_amplitude(samples: &[f64], sample_rate: f64, frequency: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let angular_step = std::f64::consts::TAU * frequency / sample_rate;
    let (in_phase, quadrature) =
        samples
            .iter()
            .enumerate()
            .fold((0.0, 0.0), |(in_phase, quadrature), (index, sample)| {
                let angle = angular_step * index as f64;
                (
                    in_phase + sample * angle.cos(),
                    quadrature + sample * angle.sin(),
                )
            });
    2.0 * in_phase.hypot(quadrature) / samples.len() as f64
}
