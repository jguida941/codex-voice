//! Background worker that records audio, runs STT, and falls back to python when needed.
//! This keeps the UI responsive while still guaranteeing a transcript even if the
//! native recorder/transcriber path hits driver issues.

use crate::audio;
use crate::log_debug;
use crate::stt;
use anyhow::{anyhow, Result};
#[cfg(test)]
use std::sync::OnceLock;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;

/// Shows whether capture was started manually or by auto mode.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VoiceCaptureTrigger {
    Manual,
    Auto,
}

/// Handle the UI uses to poll the worker thread for results.
pub struct VoiceJob {
    pub receiver: mpsc::Receiver<VoiceJobMessage>,
    pub handle: Option<thread::JoinHandle<()>>,
}

/// Messages sent from the worker back to the UI.
#[derive(Debug, PartialEq, Eq)]
pub enum VoiceJobMessage {
    Transcript {
        text: String,
        source: VoiceCaptureSource,
    },
    Empty {
        source: VoiceCaptureSource,
    },
    Error(String),
}

/// Identifies whether the Rust or Python path produced the transcript.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VoiceCaptureSource {
    Native,
    Python,
}

impl VoiceCaptureSource {
    pub fn label(self) -> &'static str {
        match self {
            VoiceCaptureSource::Native => "Rust pipeline",
            VoiceCaptureSource::Python => "Python fallback",
        }
    }
}

/// Spawn a worker thread that records audio and runs transcription.
pub fn start_voice_job(
    recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    config: crate::config::AppConfig,
) -> VoiceJob {
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        // Do the heavy work off the UI thread and send back one message.
        let message = perform_voice_capture(recorder, transcriber, &config);
        let _ = tx.send(message);
    });

    VoiceJob {
        receiver: rx,
        handle: Some(handle),
    }
}

/// Try the native path first, fall back to python if it fails.
fn perform_voice_capture(
    recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    config: &crate::config::AppConfig,
) -> VoiceJobMessage {
    let (Some(recorder), Some(transcriber)) = (recorder, transcriber) else {
        return fallback_or_error(config, "native pipeline unavailable");
    };

    match capture_voice_native(recorder, transcriber, config) {
        Ok(Some(transcript)) => VoiceJobMessage::Transcript {
            text: transcript,
            source: VoiceCaptureSource::Native,
        },
        Ok(None) => VoiceJobMessage::Empty {
            source: VoiceCaptureSource::Native,
        },
        Err(native_err) => fallback_or_error(config, &format!("{native_err:#}")),
    }
}

fn run_python_fallback(config: &crate::config::AppConfig, native_msg: &str) -> VoiceJobMessage {
    if config.no_python_fallback {
        return VoiceJobMessage::Error(format!(
            "native pipeline failed ({native_msg}); python fallback disabled (--no-python-fallback)"
        ));
    }

    log_debug(&format!(
        "Native voice capture unavailable/failed ({native_msg}). Falling back to python pipeline."
    ));
    match call_python_transcription(config) {
        Ok(pipeline) => {
            let transcript = pipeline.transcript.trim().to_string();
            if transcript.is_empty() {
                VoiceJobMessage::Empty {
                    source: VoiceCaptureSource::Python,
                }
            } else {
                VoiceJobMessage::Transcript {
                    text: transcript,
                    source: VoiceCaptureSource::Python,
                }
            }
        }
        Err(python_err) => VoiceJobMessage::Error(format!(
            "native pipeline failed ({native_msg}); python fallback failed ({python_err:#})"
        )),
    }
}

fn fallback_or_error(config: &crate::config::AppConfig, native_msg: &str) -> VoiceJobMessage {
    if config.no_python_fallback {
        VoiceJobMessage::Error(format!(
            "native pipeline failed ({native_msg}); python fallback disabled (--no-python-fallback)"
        ))
    } else {
        run_python_fallback(config, native_msg)
    }
}

fn call_python_transcription(
    config: &crate::config::AppConfig,
) -> anyhow::Result<crate::PipelineJsonResult> {
    #[cfg(test)]
    {
        if let Some(storage) = PYTHON_TRANSCRIPTION_HOOK.get() {
            let guard = storage.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(hook) = guard.as_ref() {
                return hook(config);
            }
        }
    }
    crate::run_python_transcription(config)
}

#[cfg(test)]
type PythonTranscriptionHook = Box<
    dyn Fn(&crate::config::AppConfig) -> anyhow::Result<crate::PipelineJsonResult> + Send + 'static,
>;

#[cfg(test)]
static PYTHON_TRANSCRIPTION_HOOK: OnceLock<Mutex<Option<PythonTranscriptionHook>>> =
    OnceLock::new();

#[cfg(test)]
fn set_python_transcription_hook(hook: Option<PythonTranscriptionHook>) {
    let storage = PYTHON_TRANSCRIPTION_HOOK.get_or_init(|| Mutex::new(None));
    *storage.lock().unwrap_or_else(|e| e.into_inner()) = hook;
}

/// Record audio, run Whisper, and return the trimmed transcript.
fn capture_voice_native(
    recorder: Arc<Mutex<audio::Recorder>>,
    transcriber: Arc<Mutex<stt::Transcriber>>,
    config: &crate::config::AppConfig,
) -> Result<Option<String>> {
    log_debug("capture_voice_native: Starting");
    let lang = config.lang.clone();
    let pipeline_cfg = config.voice_pipeline_config();
    let vad_cfg: audio::VadConfig = (&pipeline_cfg).into();
    let record_start = Instant::now();
    let capture = {
        let recorder_guard = recorder
            .lock()
            .map_err(|_| anyhow!("audio recorder lock poisoned"))?;
        let mut vad_engine = create_vad_engine(&pipeline_cfg);
        recorder_guard.record_with_vad(&vad_cfg, vad_engine.as_mut())
    }?;
    log_voice_metrics(&capture.metrics);
    let record_elapsed = record_start.elapsed().as_secs_f64();

    log_debug("capture_voice_native: Starting transcription");
    let stt_start = Instant::now();
    let transcript = {
        let transcriber_guard = transcriber
            .lock()
            .map_err(|_| anyhow!("transcriber lock poisoned"))?;
        // Output suppression is now handled inside transcribe() method
        transcriber_guard.transcribe(&capture.audio, &lang)?
    };
    let stt_elapsed = stt_start.elapsed().as_secs_f64();

    log_debug(&format!(
        "capture_voice_native: Transcription complete in {stt_elapsed:.2}s"
    ));

    let cleaned = transcript.trim().to_string();
    if config.log_timings {
        log_debug(&format!(
            "timing|phase=voice_capture|record_s={:.3}|stt_s={:.3}|chars={}",
            record_elapsed,
            stt_elapsed,
            cleaned.len()
        ));
    }

    if cleaned.is_empty() {
        Ok(None)
    } else {
        Ok(Some(cleaned))
    }
}

fn log_voice_metrics(metrics: &audio::CaptureMetrics) {
    log_debug(&format!(
        "voice_metrics|speech_ms={} silence_ms={} frames_processed={} frames_dropped={} reason={:?}",
        metrics.speech_ms,
        metrics.silence_tail_ms,
        metrics.frames_processed,
        metrics.frames_dropped,
        metrics.early_stop_reason
    ));
}

fn create_vad_engine(cfg: &crate::config::VoicePipelineConfig) -> Box<dyn audio::VadEngine> {
    #[cfg(feature = "vad_earshot")]
    {
        Box::new(crate::vad_earshot::EarshotVad::from_config(cfg))
    }

    #[cfg(not(feature = "vad_earshot"))]
    {
        Box::new(audio::SimpleThresholdVad::new(cfg.vad_threshold_db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::{PipelineJsonResult, PipelineMetrics};
    use clap::Parser;

    static TEST_HOOK_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_config() -> AppConfig {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.validate().expect("defaults should be valid");
        cfg
    }

    fn pipeline_result(transcript: &str) -> PipelineJsonResult {
        PipelineJsonResult {
            transcript: transcript.to_string(),
            prompt: String::new(),
            codex_output: None,
            metrics: PipelineMetrics::default(),
        }
    }

    fn with_python_hook<R>(hook: PythonTranscriptionHook, f: impl FnOnce() -> R) -> R {
        let _guard = TEST_HOOK_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        set_python_transcription_hook(Some(hook));

        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                set_python_transcription_hook(None);
            }
        }
        let _reset = Reset; // clears hook even if f() panics

        f()
    }

    #[test]
    fn voice_capture_source_labels_are_user_friendly() {
        assert_eq!(VoiceCaptureSource::Native.label(), "Rust pipeline");
        assert_eq!(VoiceCaptureSource::Python.label(), "Python fallback");
    }

    #[test]
    fn python_fallback_returns_trimmed_transcript() {
        let config = test_config();
        let message = with_python_hook(Box::new(|_| Ok(pipeline_result("  hello "))), || {
            run_python_fallback(&config, "native unavailable")
        });

        match message {
            VoiceJobMessage::Transcript { text, source } => {
                assert_eq!(text, "hello");
                assert_eq!(source, VoiceCaptureSource::Python);
            }
            other => panic!("expected transcript, got {other:?}"),
        }
    }

    #[test]
    fn python_fallback_reports_empty_transcripts() {
        let config = test_config();
        let message = with_python_hook(Box::new(|_| Ok(pipeline_result("   "))), || {
            run_python_fallback(&config, "no native path")
        });

        match message {
            VoiceJobMessage::Empty { source } => {
                assert_eq!(source, VoiceCaptureSource::Python);
            }
            other => panic!("expected empty message, got {other:?}"),
        }
    }

    #[test]
    fn python_fallback_surfaces_errors() {
        let config = test_config();
        let message = with_python_hook(Box::new(|_| Err(anyhow!("python boom"))), || {
            run_python_fallback(&config, "native blew up")
        });

        match message {
            VoiceJobMessage::Error(text) => {
                assert!(
                    text.contains("native blew up") && text.contains("python boom"),
                    "error should include both paths, got {text}"
                );
            }
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn perform_voice_capture_falls_back_when_components_missing() {
        let config = test_config();
        let message = with_python_hook(
            Box::new(|_| Ok(pipeline_result("fallback success"))),
            || perform_voice_capture(None, None, &config),
        );

        match message {
            VoiceJobMessage::Transcript { text, source } => {
                assert_eq!(text, "fallback success");
                assert_eq!(source, VoiceCaptureSource::Python);
            }
            other => panic!("expected fallback transcript, got {other:?}"),
        }
    }

    #[test]
    fn error_when_fallback_disabled_and_native_unavailable() {
        let mut config = test_config();
        config.no_python_fallback = true;
        let message = perform_voice_capture(None, None, &config);

        match message {
            VoiceJobMessage::Error(text) => {
                assert!(
                    text.contains("python fallback disabled"),
                    "expected disable hint, got {text}"
                );
            }
            other => panic!("expected error, got {other:?}"),
        }
    }
}
