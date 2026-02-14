//! Voice-capture manager that centralizes lifecycle, config updates, and cancellation.

use anyhow::{anyhow, Result};
use crossbeam_channel::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use voiceterm::{
    audio, config::AppConfig, log_debug, stt, voice, VoiceCaptureSource, VoiceCaptureTrigger,
    VoiceJobMessage,
};

use crate::status_line::{Pipeline, RecordingState, StatusLineState};
use crate::writer::{send_enhanced_status, set_status, WriterMessage};

use super::drain::clear_capture_metrics;
use super::pipeline::using_native_pipeline;
use super::STATUS_TOAST_SECS;

struct VoiceStartInfo {
    pipeline_display: &'static str,
    source: VoiceCaptureSource,
    fallback_note: Option<String>,
}

pub(crate) struct VoiceManager {
    config: AppConfig,
    recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    job: Option<voice::VoiceJob>,
    cancel_pending: bool,
    active_source: Option<VoiceCaptureSource>,
    live_meter: audio::LiveMeter,
}

impl VoiceManager {
    pub(crate) fn new(config: AppConfig) -> Self {
        Self {
            config,
            recorder: None,
            transcriber: None,
            job: None,
            cancel_pending: false,
            active_source: None,
            live_meter: audio::LiveMeter::new(),
        }
    }

    pub(crate) fn adjust_sensitivity(&mut self, delta_db: f32) -> f32 {
        const MIN_DB: f32 = -80.0;
        const MAX_DB: f32 = -10.0;
        let mut next = self.config.voice_vad_threshold_db + delta_db;
        next = next.clamp(MIN_DB, MAX_DB);
        self.config.voice_vad_threshold_db = next;
        next
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.job.is_none()
    }

    pub(crate) fn active_source(&self) -> Option<VoiceCaptureSource> {
        self.active_source
    }

    pub(crate) fn meter(&self) -> audio::LiveMeter {
        self.live_meter.clone()
    }

    /// Cancel any running voice capture. Returns true if a capture was cancelled.
    pub(crate) fn cancel_capture(&mut self) -> bool {
        if let Some(ref job) = self.job {
            job.request_stop();
            self.cancel_pending = true;
            log_debug("voice capture cancel requested");
            true
        } else {
            false
        }
    }

    /// Request early stop of voice capture (stop recording and process what was captured).
    /// Returns true if a capture was running and will be stopped.
    pub(crate) fn request_early_stop(&mut self) -> bool {
        if let Some(ref job) = self.job {
            job.request_stop();
            log_debug("voice capture early stop requested");
            true
        } else {
            false
        }
    }

    fn start_capture(&mut self, trigger: VoiceCaptureTrigger) -> Result<Option<VoiceStartInfo>> {
        if self.job.is_some() {
            return Ok(None);
        }

        let transcriber = self.get_transcriber()?;
        if transcriber.is_none() {
            log_debug(
                "No native Whisper model configured; using python fallback for voice capture.",
            );
            if self.config.no_python_fallback {
                return Err(anyhow!(
                    "Native Whisper model not configured and --no-python-fallback is set."
                ));
            }
        }

        let mut fallback_note: Option<String> = None;
        let recorder = if transcriber.is_some() {
            match self.get_recorder() {
                Ok(recorder) => Some(recorder),
                Err(err) => {
                    if self.config.no_python_fallback {
                        return Err(anyhow!(
                            "Audio recorder unavailable and --no-python-fallback is set: {err:#}"
                        ));
                    }
                    log_debug(&format!(
                        "Audio recorder unavailable ({err:#}); falling back to python pipeline."
                    ));
                    fallback_note =
                        Some("Recorder unavailable; falling back to python pipeline.".into());
                    None
                }
            }
        } else {
            None
        };

        let using_native = using_native_pipeline(transcriber.is_some(), recorder.is_some());
        let source = if using_native {
            VoiceCaptureSource::Native
        } else {
            VoiceCaptureSource::Python
        };
        let job = voice::start_voice_job(
            recorder,
            transcriber.clone(),
            self.config.clone(),
            Some(self.live_meter.clone()),
        );
        self.job = Some(job);
        self.cancel_pending = false;
        self.active_source = Some(source);

        let pipeline_label = if using_native {
            "Rust pipeline"
        } else {
            "Python pipeline"
        };
        let pipeline_display = if using_native { "Rust" } else { "Python" };

        let status = match trigger {
            VoiceCaptureTrigger::Manual => "manual",
            VoiceCaptureTrigger::Auto => "auto",
        };
        log_debug(&format!(
            "voice capture started ({status}) using {pipeline_label}"
        ));

        Ok(Some(VoiceStartInfo {
            pipeline_display,
            source,
            fallback_note,
        }))
    }

    pub(crate) fn poll_message(&mut self) -> Option<VoiceJobMessage> {
        let job = self.job.as_mut()?;
        match job.receiver.try_recv() {
            Ok(message) => {
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
                self.job = None;
                self.active_source = None;
                if self.cancel_pending {
                    self.cancel_pending = false;
                    log_debug("voice capture cancelled; dropping message");
                    None
                } else {
                    Some(message)
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
                self.job = None;
                self.active_source = None;
                let was_cancelled = self.cancel_pending;
                self.cancel_pending = false;
                if was_cancelled {
                    log_debug("voice capture cancelled; worker disconnected");
                    None
                } else {
                    Some(VoiceJobMessage::Error(
                        "voice capture worker disconnected unexpectedly".to_string(),
                    ))
                }
            }
        }
    }

    fn get_recorder(&mut self) -> Result<Arc<Mutex<audio::Recorder>>> {
        if self.recorder.is_none() {
            let recorder = audio::Recorder::new(self.config.input_device.as_deref())?;
            self.recorder = Some(Arc::new(Mutex::new(recorder)));
        }
        Ok(self
            .recorder
            .as_ref()
            .expect("recorder initialized")
            .clone())
    }

    fn get_transcriber(&mut self) -> Result<Option<Arc<Mutex<stt::Transcriber>>>> {
        if self.transcriber.is_none() {
            let Some(model_path) = self.config.whisper_model_path.clone() else {
                return Ok(None);
            };
            let transcriber = stt::Transcriber::new(&model_path)?;
            self.transcriber = Some(Arc::new(Mutex::new(transcriber)));
        }
        Ok(self.transcriber.as_ref().cloned())
    }
}

pub(crate) fn start_voice_capture(
    voice_manager: &mut VoiceManager,
    trigger: VoiceCaptureTrigger,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
) -> Result<()> {
    match voice_manager.start_capture(trigger)? {
        Some(info) => {
            status_state.recording_state = RecordingState::Recording;
            clear_capture_metrics(status_state);
            status_state.recording_duration = Some(0.0);
            status_state.pipeline = match info.source {
                VoiceCaptureSource::Native => Pipeline::Rust,
                VoiceCaptureSource::Python => Pipeline::Python,
            };
            if trigger == VoiceCaptureTrigger::Auto {
                status_state.message.clear();
                send_enhanced_status(writer_tx, status_state);
                return Ok(());
            }
            let mode_label = match trigger {
                VoiceCaptureTrigger::Manual => "Manual Mode",
                VoiceCaptureTrigger::Auto => "Auto Mode",
            };
            let mut status = format!("Listening {mode_label} ({})", info.pipeline_display);
            if let Some(note) = info.fallback_note {
                status.push(' ');
                status.push_str(&note);
            }
            set_status(
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                &status,
                None,
            );
            Ok(())
        }
        None => {
            if trigger == VoiceCaptureTrigger::Manual {
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                    "Voice capture already running",
                    Some(Duration::from_secs(STATUS_TOAST_SECS)),
                );
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use voiceterm::config::AppConfig;

    #[test]
    fn voice_manager_clamps_sensitivity() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        assert_eq!(manager.adjust_sensitivity(1000.0), -10.0);
        assert_eq!(manager.adjust_sensitivity(-1000.0), -80.0);
    }

    #[test]
    fn voice_manager_reports_idle_and_source() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        assert!(manager.is_idle());
        manager.active_source = Some(VoiceCaptureSource::Python);
        assert_eq!(manager.active_source(), Some(VoiceCaptureSource::Python));
    }

    #[test]
    fn cancel_capture_suppresses_voice_message() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        let (tx, rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_worker = stop_flag.clone();
        let handle = thread::spawn(move || {
            while !stop_flag_worker.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(5));
            }
            let _ = tx.send(VoiceJobMessage::Empty {
                source: VoiceCaptureSource::Native,
                metrics: None,
            });
        });
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: Some(handle),
            stop_flag: stop_flag.clone(),
        });
        manager.active_source = Some(VoiceCaptureSource::Native);

        assert!(manager.cancel_capture());
        assert!(stop_flag.load(Ordering::Relaxed));

        for _ in 0..50 {
            manager.poll_message();
            if manager.job.is_none() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(manager.job.is_none());
        assert!(!manager.cancel_pending);
    }

    #[test]
    fn voice_manager_is_idle_false_when_job_present() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        let (_tx, rx) = mpsc::channel();
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });
        assert!(!manager.is_idle());
    }

    #[test]
    fn voice_manager_request_early_stop_sets_flag() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        assert!(!manager.request_early_stop());

        let (_tx, rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: stop_flag.clone(),
        });
        assert!(manager.request_early_stop());
        assert!(stop_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn voice_manager_start_capture_errors_without_fallback() {
        let mut config = AppConfig::parse_from(["test"]);
        config.no_python_fallback = true;
        let mut manager = VoiceManager::new(config);
        assert!(manager.start_capture(VoiceCaptureTrigger::Manual).is_err());
    }

    #[test]
    fn voice_manager_get_transcriber_errors_on_missing_model() {
        let mut config = AppConfig::parse_from(["test"]);
        config.whisper_model_path = Some("/no/such/model.bin".to_string());
        let mut manager = VoiceManager::new(config);
        assert!(manager.get_transcriber().is_err());
    }

    #[test]
    fn start_voice_capture_reports_running_job_on_manual_only() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        let (_tx, rx) = mpsc::channel();
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });

        let (writer_tx, writer_rx) = crossbeam_channel::unbounded();
        let mut deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        start_voice_capture(
            &mut manager,
            VoiceCaptureTrigger::Manual,
            &writer_tx,
            &mut deadline,
            &mut current_status,
            &mut status_state,
        )
        .expect("start capture manual");

        let msg = writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message");
        match msg {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("already running"));
            }
            _ => panic!("unexpected writer message"),
        }
        while writer_rx.try_recv().is_ok() {}

        start_voice_capture(
            &mut manager,
            VoiceCaptureTrigger::Auto,
            &writer_tx,
            &mut deadline,
            &mut current_status,
            &mut status_state,
        )
        .expect("start capture auto");

        assert!(writer_rx.try_recv().is_err());
    }
}
