//! Full pipeline latency measurement harness for Phase 2B measurement gate.
//!
//! This binary instruments the complete voice→Codex flow to identify bottlenecks:
//! - Voice capture (record + STT)
//! - Codex API call
//! - Total round-trip latency
//!
//! Outputs structured metrics for analysis in LATENCY_MEASUREMENTS.md.

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use rust_tui::audio;
use rust_tui::codex::{BackendEventKind, CliBackend, CodexBackend, CodexRequest};
use rust_tui::config::AppConfig;
use rust_tui::stt;
use rust_tui::voice::{self, VoiceJobMessage};
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Measure end-to-end latency for voice→Codex pipeline
#[derive(Debug, Parser)]
#[command(about = "Measure voice→Codex round-trip latency for Phase 2B analysis")]
struct Args {
    /// Human-readable label for this measurement run
    #[arg(long, default_value = "measurement")]
    label: String,

    /// Number of measurements to collect
    #[arg(long, default_value_t = 1)]
    count: usize,

    /// Skip Codex call and only measure voice pipeline
    #[arg(long)]
    voice_only: bool,

    /// Use synthetic audio instead of real microphone (requires --speech-ms and --silence-ms)
    #[arg(long)]
    synthetic: bool,

    /// Speech duration for synthetic audio (milliseconds)
    #[arg(long)]
    speech_ms: Option<u64>,

    /// Silence duration for synthetic audio (milliseconds)
    #[arg(long)]
    silence_ms: Option<u64>,
}

#[derive(Debug)]
struct LatencyMeasurement {
    label: String,
    voice_capture_ms: u64,
    voice_stt_ms: u64,
    voice_total_ms: u64,
    codex_ms: Option<u64>,
    total_ms: u64,
    transcript_chars: usize,
    codex_output_chars: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.synthetic {
        if args.speech_ms.is_none() || args.silence_ms.is_none() {
            bail!("--synthetic requires both --speech-ms and --silence-ms");
        }
    }

    // Parse base config from environment/defaults and enable timing logs
    let mut config = AppConfig::parse_from(Vec::<String>::new());
    config.validate()?; // This auto-discovers Whisper model and validates all settings
    config.log_timings = true; // Enable detailed timing logs for accurate breakdown
    eprintln!("PTY enabled: {}", config.persistent_codex);

    let measurements = if args.synthetic {
        collect_synthetic_measurements(&args, &config)?
    } else {
        collect_real_measurements(&args, &config)?
    };

    print_measurements(&measurements);
    print_analysis(&measurements, args.voice_only);

    Ok(())
}

fn collect_real_measurements(args: &Args, config: &AppConfig) -> Result<Vec<LatencyMeasurement>> {
    let mut measurements = Vec::with_capacity(args.count);

    // Initialize heavy resources once
    let recorder = audio::Recorder::new(config.input_device.as_deref())
        .context("failed to initialize audio recorder")?;
    let recorder = Arc::new(Mutex::new(recorder));

    let transcriber = if let Some(model_path) = &config.whisper_model_path {
        let t = stt::Transcriber::new(model_path).context("failed to load Whisper model")?;
        Some(Arc::new(Mutex::new(t)))
    } else {
        eprintln!("Warning: No Whisper model configured, using Python fallback");
        None
    };

    let backend: Arc<dyn CodexBackend> = Arc::new(CliBackend::new(config.clone()));

    for i in 1..=args.count {
        eprintln!("\n=== Measurement {}/{} ===", i, args.count);
        eprintln!("Press Ctrl+R when ready to speak...");

        let measurement = measure_single_run(
            &args.label,
            Some(recorder.clone()),
            transcriber.clone(),
            backend.as_ref(),
            config,
            args.voice_only,
        )?;

        measurements.push(measurement);
    }

    Ok(measurements)
}

fn collect_synthetic_measurements(
    args: &Args,
    config: &AppConfig,
) -> Result<Vec<LatencyMeasurement>> {
    let speech_ms = args.speech_ms.unwrap();
    let silence_ms = args.silence_ms.unwrap();

    let mut measurements = Vec::with_capacity(args.count);

    let transcriber = if let Some(model_path) = &config.whisper_model_path {
        let t = stt::Transcriber::new(model_path).context("failed to load Whisper model")?;
        Some(Arc::new(Mutex::new(t)))
    } else {
        eprintln!("Warning: No Whisper model configured, using Python fallback");
        None
    };

    let backend: Arc<dyn CodexBackend> = Arc::new(CliBackend::new(config.clone()));

    for i in 1..=args.count {
        eprintln!("\n=== Measurement {}/{} ===", i, args.count);
        eprintln!(
            "Running synthetic clip: {}ms speech + {}ms silence",
            speech_ms, silence_ms
        );

        let measurement = measure_synthetic_run(
            &args.label,
            speech_ms,
            silence_ms,
            transcriber.clone(),
            backend.as_ref(),
            config,
            args.voice_only,
        )?;

        measurements.push(measurement);
    }

    Ok(measurements)
}

fn measure_single_run(
    label: &str,
    recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    backend: &dyn CodexBackend,
    config: &AppConfig,
    voice_only: bool,
) -> Result<LatencyMeasurement> {
    eprintln!("Starting voice capture...");
    let t0 = Instant::now();

    let job = voice::start_voice_job(recorder, transcriber, config.clone());
    let message = wait_for_voice_job(job)?;

    let t1 = Instant::now();
    let voice_total_ms = t1.duration_since(t0).as_millis() as u64;

    let transcript = match message {
        VoiceJobMessage::Transcript { text, .. } => text,
        VoiceJobMessage::Empty { .. } => {
            bail!("Voice capture returned empty transcript");
        }
        VoiceJobMessage::Error(err) => {
            bail!("Voice capture failed: {}", err);
        }
    };

    eprintln!("Voice capture complete: {} ms", voice_total_ms);
    eprintln!("Transcript: {}", transcript);

    // Extract capture and STT timing from logs if available
    let (voice_capture_ms, voice_stt_ms) = extract_voice_timings(voice_total_ms);

    let (codex_ms, codex_output_chars, total_ms) = if voice_only {
        (None, 0, voice_total_ms)
    } else {
        eprintln!("\nStarting Codex call...");
        let request = CodexRequest::chat(transcript.clone());

        let job = match backend.start(request) {
            Ok(job) => job,
            Err(err) => bail!("Failed to start Codex job: {:?}", err),
        };

        let t2 = Instant::now();
        let codex_output = wait_for_codex_job(job)?;
        let t3 = Instant::now();

        let codex_elapsed_ms = t3.duration_since(t2).as_millis() as u64;
        let total_elapsed_ms = t3.duration_since(t0).as_millis() as u64;

        eprintln!("Codex complete: {} ms", codex_elapsed_ms);
        eprintln!(
            "Output preview: {}...",
            codex_output.chars().take(100).collect::<String>()
        );

        (
            Some(codex_elapsed_ms),
            codex_output.len(),
            total_elapsed_ms,
        )
    };

    Ok(LatencyMeasurement {
        label: label.to_string(),
        voice_capture_ms,
        voice_stt_ms,
        voice_total_ms,
        codex_ms,
        total_ms,
        transcript_chars: transcript.len(),
        codex_output_chars,
    })
}

fn measure_synthetic_run(
    label: &str,
    speech_ms: u64,
    silence_ms: u64,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    backend: &dyn CodexBackend,
    config: &AppConfig,
    voice_only: bool,
) -> Result<LatencyMeasurement> {
    use std::f32::consts::PI;

    // Generate synthetic audio (440 Hz sine wave)
    let sample_rate = config.voice_pipeline_config().sample_rate;
    let speech_samples = (speech_ms * sample_rate as u64 / 1000) as usize;
    let silence_samples = (silence_ms * sample_rate as u64 / 1000) as usize;

    let mut samples = Vec::with_capacity(speech_samples + silence_samples);
    for n in 0..speech_samples {
        let t = n as f32 / sample_rate as f32;
        let sample = (2.0 * PI * 440.0 * t).sin() * 0.4;
        samples.push(sample);
    }
    samples.extend(std::iter::repeat_n(0.0, silence_samples));

    let t0 = Instant::now();

    // Run offline capture
    let pipeline_cfg = config.voice_pipeline_config();
    let vad_cfg: audio::VadConfig = (&pipeline_cfg).into();
    let mut vad_engine = create_vad_engine(&pipeline_cfg);
    let capture = audio::offline_capture_from_pcm(&samples, &vad_cfg, vad_engine.as_mut());

    let t_capture = Instant::now();
    let voice_capture_ms = t_capture.duration_since(t0).as_millis() as u64;

    // Run STT
    let transcript = if let Some(transcriber) = transcriber {
        let guard = transcriber
            .lock()
            .map_err(|_| anyhow!("transcriber lock poisoned"))?;
        guard.transcribe(&capture.audio, &config.lang)?
    } else {
        bail!("Synthetic mode requires native Whisper model");
    };

    let t1 = Instant::now();
    let voice_stt_ms = t1.duration_since(t_capture).as_millis() as u64;
    let voice_total_ms = t1.duration_since(t0).as_millis() as u64;

    eprintln!("Voice capture: {} ms", voice_capture_ms);
    eprintln!("STT: {} ms", voice_stt_ms);
    eprintln!("Transcript: {}", transcript);

    let (codex_ms, codex_output_chars, total_ms) = if voice_only {
        (None, 0, voice_total_ms)
    } else {
        eprintln!("\nStarting Codex call...");
        let request = CodexRequest::chat(transcript.clone());

        let job = match backend.start(request) {
            Ok(job) => job,
            Err(err) => bail!("Failed to start Codex job: {:?}", err),
        };

        let t2 = Instant::now();
        let codex_output = wait_for_codex_job(job)?;
        let t3 = Instant::now();

        let codex_elapsed_ms = t3.duration_since(t2).as_millis() as u64;
        let total_elapsed_ms = t3.duration_since(t0).as_millis() as u64;

        eprintln!("Codex complete: {} ms", codex_elapsed_ms);

        (
            Some(codex_elapsed_ms),
            codex_output.len(),
            total_elapsed_ms,
        )
    };

    Ok(LatencyMeasurement {
        label: label.to_string(),
        voice_capture_ms,
        voice_stt_ms,
        voice_total_ms,
        codex_ms,
        total_ms,
        transcript_chars: transcript.len(),
        codex_output_chars,
    })
}

fn wait_for_voice_job(mut job: voice::VoiceJob) -> Result<VoiceJobMessage> {
    loop {
        match job.receiver.try_recv() {
            Ok(message) => {
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
                return Ok(message);
            }
            Err(TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(TryRecvError::Disconnected) => {
                bail!("Voice job worker disconnected unexpectedly");
            }
        }
    }
}

fn wait_for_codex_job(mut job: rust_tui::codex::BackendJob) -> Result<String> {
    let mut output_lines = Vec::new();

    loop {
        match job.try_recv_signal() {
            Ok(()) => {
                let events = job.drain_events();
                for event in events {
                    match event.kind {
                        BackendEventKind::Finished { lines, .. } => {
                            output_lines = lines;
                            if let Some(handle) = job.take_handle() {
                                let _ = handle.join();
                            }
                            return Ok(output_lines.join("\n"));
                        }
                        BackendEventKind::FatalError { message, .. } => {
                            bail!("Codex failed: {}", message);
                        }
                        BackendEventKind::Canceled { .. } => {
                            bail!("Codex job was canceled");
                        }
                        _ => {}
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(TryRecvError::Disconnected) => {
                // Worker finished, drain remaining events
                let events = job.drain_events();
                for event in events {
                    if let BackendEventKind::Finished { lines, .. } = event.kind {
                        output_lines = lines;
                    }
                }
                if output_lines.is_empty() {
                    bail!("Codex worker disconnected without finishing");
                }
                return Ok(output_lines.join("\n"));
            }
        }
    }
}

fn extract_voice_timings(total_ms: u64) -> (u64, u64) {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    // Try to parse actual timings from log file
    if let Ok(log_path) = std::env::var("TMPDIR") {
        let log_file = std::path::Path::new(&log_path).join("codex_voice_tui.log");
        if let Ok(file) = File::open(log_file) {
            let reader = BufReader::new(file);
            // Collect last 100 lines and search in reverse
            let lines: Vec<_> = reader.lines().filter_map(Result::ok).collect();
            for line in lines.iter().rev().take(100) {
                if line.contains("timing|phase=voice_capture|") {
                    // Parse: timing|phase=voice_capture|record_s=1.234|stt_s=0.567|chars=42
                    let mut record_s = None;
                    let mut stt_s = None;
                    for part in line.split('|') {
                        if let Some(val) = part.strip_prefix("record_s=") {
                            record_s = val.parse::<f64>().ok();
                        } else if let Some(val) = part.strip_prefix("stt_s=") {
                            stt_s = val.parse::<f64>().ok();
                        }
                    }
                    if let (Some(record), Some(stt)) = (record_s, stt_s) {
                        return ((record * 1000.0) as u64, (stt * 1000.0) as u64);
                    }
                }
            }
        }
    }

    // Fallback: Don't guess, just report total only
    // This makes it clear we don't have breakdown data
    (total_ms, 0)
}

fn create_vad_engine(cfg: &rust_tui::config::VoicePipelineConfig) -> Box<dyn audio::VadEngine> {
    use rust_tui::config::VadEngineKind;

    match cfg.vad_engine {
        VadEngineKind::Simple => Box::new(audio::SimpleThresholdVad::new(cfg.vad_threshold_db)),
        VadEngineKind::Earshot => {
            #[cfg(feature = "vad_earshot")]
            {
                Box::new(rust_tui::vad_earshot::EarshotVad::from_config(cfg))
            }
            #[cfg(not(feature = "vad_earshot"))]
            {
                unreachable!("earshot VAD requested without 'vad_earshot' feature")
            }
        }
    }
}

fn print_measurements(measurements: &[LatencyMeasurement]) {
    println!("\n=== LATENCY MEASUREMENTS ===\n");

    // Check if we have timing breakdown data
    let has_breakdown = measurements.iter().any(|m| m.voice_stt_ms > 0);
    if !has_breakdown {
        println!("Note: Detailed capture/STT breakdown unavailable (enable --log-timings for precise split)\n");
    }

    println!("| label | voice_capture_ms | voice_stt_ms | voice_total_ms | codex_ms | total_ms | transcript_chars | codex_output_chars |");
    println!("|-------|------------------|--------------|----------------|----------|----------|------------------|--------------------|");

    for m in measurements {
        let codex_str = m
            .codex_ms
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            m.label,
            m.voice_capture_ms,
            m.voice_stt_ms,
            m.voice_total_ms,
            codex_str,
            m.total_ms,
            m.transcript_chars,
            m.codex_output_chars
        );
    }
    println!();
}

fn print_analysis(measurements: &[LatencyMeasurement], voice_only: bool) {
    if measurements.is_empty() {
        return;
    }

    println!("=== ANALYSIS ===\n");

    let avg_voice = measurements.iter().map(|m| m.voice_total_ms).sum::<u64>() as f64
        / measurements.len() as f64;

    println!("Voice Pipeline:");
    println!("  Average total: {:.1} ms", avg_voice);

    if !voice_only && measurements.iter().any(|m| m.codex_ms.is_some()) {
        let codex_times: Vec<u64> = measurements.iter().filter_map(|m| m.codex_ms).collect();
        if !codex_times.is_empty() {
            let avg_codex = codex_times.iter().sum::<u64>() as f64 / codex_times.len() as f64;
            let avg_total = measurements.iter().map(|m| m.total_ms).sum::<u64>() as f64
                / measurements.len() as f64;

            println!("\nCodex API:");
            println!("  Average: {:.1} ms", avg_codex);

            println!("\nTotal Round-Trip:");
            println!("  Average: {:.1} ms", avg_total);

            let voice_pct = (avg_voice / avg_total) * 100.0;
            let codex_pct = (avg_codex / avg_total) * 100.0;

            println!("\nBottleneck Analysis:");
            println!("  Voice:  {:.1}% of total time", voice_pct);
            println!("  Codex:  {:.1}% of total time", codex_pct);

            println!("\nRecommendations:");
            if codex_pct > 70.0 {
                println!("  ⚠️  Codex API is the primary bottleneck ({:.1}%)", codex_pct);
                println!(
                    "  → Voice optimization (Phase 2B) would save <{:.0}% of total latency",
                    voice_pct
                );
                println!("  → Consider deferring Phase 2B until Codex latency is improved");
            } else if voice_pct > 50.0 {
                println!("  ✓ Voice is significant bottleneck ({:.1}%)", voice_pct);
                println!("  → Phase 2B streaming architecture justified");
                println!("  → Target: reduce voice latency to <750ms");
            } else {
                println!("  ~ Both components contribute roughly equally");
                println!("  → Phase 2B may provide noticeable improvement");
            }

            if avg_voice < 750.0 {
                println!("\n  ✓ Voice latency already meets <750ms target");
            } else {
                println!(
                    "\n  ⚠️  Voice latency ({:.1}ms) exceeds 750ms target by {:.0}ms",
                    avg_voice,
                    avg_voice - 750.0
                );
            }
        }
    }
}
