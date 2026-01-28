use super::*;
use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
#[cfg(not(test))]
use crossbeam_channel::{bounded, RecvTimeoutError};
use std::sync::atomic::AtomicBool;
#[cfg(not(test))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Wraps the system input device abstraction so the rest of the app can ask for
/// "speech-ready" samples without touching cpal or thinking about sample rates.
pub struct Recorder {
    device: cpal::Device,
}

impl Recorder {
    /// List microphone names so the CLI can expose a human-friendly selector.
    pub fn list_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let devices = host.input_devices().context("no input devices available")?;
        let mut names = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                names.push(name);
            }
        }
        Ok(names)
    }

    /// Create a recorder, optionally forcing a specific device so users can pick
    /// the right microphone when a laptop exposes multiple inputs.
    pub fn new(preferred_device: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = match preferred_device {
            Some(name) => {
                let mut devices = host.input_devices().context("no input devices available")?;
                devices
                    .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                    .ok_or_else(|| anyhow!("input device '{name}' not found"))?
            }
            None => host
                .default_input_device()
                .context("no default input device available")?,
        };
        Ok(Self { device })
    }

    /// Get the name of the active recording device.
    pub fn device_name(&self) -> String {
        self.device
            .name()
            .unwrap_or_else(|_| "Unknown Device".to_string())
    }

    /// Record audio for `seconds`, normalize the incoming format, and return
    /// 16 kHz mono data that Whisper can consume directly.
    pub fn record(&self, seconds: u64) -> Result<Vec<f32>> {
        // Get the device's default config so we know the native format and channel count.
        let default_config = self.device.default_input_config()?;
        let format = default_config.sample_format();
        let device_config: StreamConfig = default_config.clone().into();
        let device_sample_rate = device_config.sample_rate.0;
        let channels = usize::from(device_config.channels.max(1));
        let device_name = self
            .device
            .name()
            .unwrap_or_else(|_| "unknown input device".to_string());

        log_debug(&format!(
            "Recorder config: format={format:?} sample_rate={device_sample_rate}Hz channels={channels}"
        ));

        // cpal delivers samples on a callback thread; collect them in a shared
        // buffer so we can keep ownership on the caller side.
        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let buffer_clone = buffer.clone();

        // Keep the error callback quiet in the UI and mirror issues into the log.
        let err_fn = |err| log_debug(&format!("audio_stream_error: {err}"));

        // Convert every supported sample type to f32 up front so the rest of the
        // pipeline can stay format-agnostic.
        let stream = match format {
            SampleFormat::F32 => self.device.build_input_stream(
                &device_config,
                move |data: &[f32], _| {
                    if let Ok(mut buf) = buffer_clone.lock() {
                        append_downmixed_samples(&mut buf, data, channels, |sample| sample);
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => self.device.build_input_stream(
                &device_config,
                move |data: &[i16], _| {
                    if let Ok(mut buf) = buffer_clone.lock() {
                        append_downmixed_samples(&mut buf, data, channels, |sample| {
                            sample as f32 / 32_768.0_f32
                        });
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::U16 => self.device.build_input_stream(
                &device_config,
                move |data: &[u16], _| {
                    if let Ok(mut buf) = buffer_clone.lock() {
                        append_downmixed_samples(&mut buf, data, channels, |sample| {
                            (sample as f32 - 32_768.0_f32) / 32_768.0_f32
                        });
                    }
                },
                err_fn,
                None,
            )?,
            other => return Err(anyhow!("unsupported sample format: {other:?}")),
        };

        stream.play()?;
        std::thread::sleep(Duration::from_secs(seconds));
        if let Err(err) = stream.pause() {
            log_debug(&format!("failed to pause audio stream: {err}"));
        }
        drop(stream);

        let samples = buffer.lock().unwrap();

        if samples.is_empty() {
            return Err(anyhow!(
                "no samples captured from '{device_name}'; check microphone permissions and availability"
            ));
        }

        // Transcription assumes 16 kHz mono, so resample if the hardware rate differs.
        let processed = resample_to_target_rate(&samples, device_sample_rate);
        Ok(processed)
    }

    #[cfg(not(test))]
    pub fn record_with_vad(
        &self,
        cfg: &VadConfig,
        vad: &mut dyn VadEngine,
        stop_flag: Option<Arc<AtomicBool>>,
    ) -> Result<CaptureResult> {
        record_with_vad_impl(self, cfg, vad, stop_flag)
    }

    #[cfg(test)]
    pub fn record_with_vad(
        &self,
        _cfg: &VadConfig,
        _vad: &mut dyn VadEngine,
        _stop_flag: Option<Arc<AtomicBool>>,
    ) -> Result<CaptureResult> {
        Ok(CaptureResult {
            audio: Vec::new(),
            metrics: CaptureMetrics::default(),
        })
    }

    #[cfg(test)]
    pub(super) fn new_for_tests() -> Option<Self> {
        let host = cpal::default_host();
        host.default_input_device().map(|device| Self { device })
    }
}

#[cfg(not(test))]
fn record_with_vad_impl(
    recorder: &Recorder,
    cfg: &VadConfig,
    vad: &mut dyn VadEngine,
    stop_flag: Option<Arc<AtomicBool>>,
) -> Result<CaptureResult> {
    let default_config = recorder.device.default_input_config()?;
    let format = default_config.sample_format();
    let device_config: StreamConfig = default_config.clone().into();
    let device_sample_rate = device_config.sample_rate.0;
    let channels = usize::from(device_config.channels.max(1));
    let frame_ms = cfg.frame_ms.clamp(5, 120);
    let device_frame_samples = ((device_sample_rate as u64 * frame_ms) / 1000).max(1) as usize;
    let target_frame_samples = ((cfg.sample_rate as u64 * frame_ms) / 1000).max(1) as usize;
    let (sender, receiver) = bounded::<Vec<f32>>(cfg.channel_capacity.max(1));
    let dropped = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(Mutex::new(FrameDispatcher::new(
        device_frame_samples,
        sender,
        dropped.clone(),
    )));

    let err_fn = |err| log_debug(&format!("audio_stream_error: {err}"));
    let stream = match format {
        SampleFormat::F32 => {
            let dispatcher = dispatcher.clone();
            recorder.device.build_input_stream(
                &device_config,
                move |data: &[f32], _| {
                    if let Ok(mut pump) = dispatcher.lock() {
                        pump.push(data, channels, |sample| sample);
                    }
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::I16 => {
            let dispatcher = dispatcher.clone();
            recorder.device.build_input_stream(
                &device_config,
                move |data: &[i16], _| {
                    if let Ok(mut pump) = dispatcher.lock() {
                        pump.push(data, channels, |sample| sample as f32 / 32_768.0);
                    }
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::U16 => {
            let dispatcher = dispatcher.clone();
            recorder.device.build_input_stream(
                &device_config,
                move |data: &[u16], _| {
                    if let Ok(mut pump) = dispatcher.lock() {
                        pump.push(data, channels, |sample| {
                            (sample as f32 - 32_768.0) / 32_768.0
                        });
                    }
                },
                err_fn,
                None,
            )?
        }
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream.play()?;

    let mut accumulator = FrameAccumulator::from_config(cfg);
    let mut state = CaptureState::new(cfg, frame_ms);
    let mut metrics = CaptureMetrics::default();
    let mut stop_reason = StopReason::MaxDuration;
    let wait_time = Duration::from_millis(frame_ms);

    while state.total_ms() < cfg.max_recording_duration_ms {
        // Check for manual stop signal
        if let Some(ref flag) = stop_flag {
            if flag.load(Ordering::Relaxed) {
                stop_reason = StopReason::ManualStop;
                break;
            }
        }
        match receiver.recv_timeout(wait_time) {
            Ok(frame) => {
                let target_frame = convert_frame_to_target(
                    frame,
                    device_sample_rate,
                    cfg.sample_rate,
                    target_frame_samples,
                );
                if target_frame.is_empty() {
                    continue;
                }

                let decision = vad.process_frame(&target_frame);
                metrics.frames_processed += 1;

                let label = FrameLabel::from(decision);
                accumulator.push_frame(target_frame, label);
                if let Some(reason) = state.on_frame(label) {
                    stop_reason = reason;
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(reason) = state.on_timeout() {
                    stop_reason = reason;
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                stop_reason = StopReason::Error("audio stream disconnected".to_string());
                break;
            }
        }
    }

    if let Err(err) = stream.pause() {
        log_debug(&format!("failed to pause audio stream: {err}"));
    }
    drop(stream);

    metrics.speech_ms = state.speech_ms();
    metrics.silence_tail_ms = state.silence_tail_ms();
    metrics.frames_dropped = dropped.load(Ordering::Relaxed);
    metrics.early_stop_reason = stop_reason;
    metrics.capture_ms = state.total_ms();

    if accumulator.is_empty() {
        if matches!(metrics.early_stop_reason, StopReason::ManualStop) {
            return Ok(CaptureResult {
                audio: Vec::new(),
                metrics,
            });
        }
        return Err(anyhow!(
            "no samples captured; check microphone permissions and availability"
        ));
    }

    let audio = accumulator.into_audio(&metrics.early_stop_reason);

    Ok(CaptureResult { audio, metrics })
}
