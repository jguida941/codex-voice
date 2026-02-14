//! Regression tests that lock configuration defaults, validation rules, and env parsing.

use super::defaults::{
    default_term, MAX_CAPTURE_HARD_LIMIT_MS, MAX_CODEX_ARGS, MAX_CODEX_ARG_BYTES,
};
use super::validation::{
    canonical_repo_root, canonicalize_within_repo, discover_default_whisper_model, sanitize_binary,
};
use super::{default_vad_engine, AppConfig, VadEngineKind};
use clap::Parser;
use std::fs;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, path::Path};

fn base_voice_config() -> AppConfig {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.voice_max_capture_ms = 1000;
    cfg.voice_silence_tail_ms = 200;
    cfg.voice_min_speech_ms_before_stt_start = 50;
    cfg.voice_lookback_ms = 0;
    cfg.voice_buffer_ms = 1000;
    cfg.voice_stt_timeout_ms = 1000;
    cfg
}

fn base_voice_config_with_capture(max_capture_ms: u64) -> AppConfig {
    let mut cfg = base_voice_config();
    cfg.voice_max_capture_ms = max_capture_ms;
    cfg.voice_buffer_ms = max_capture_ms;
    cfg.voice_stt_timeout_ms = max_capture_ms;
    cfg
}

#[test]
fn rejects_seconds_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "0"]);
    assert!(cfg.validate().is_err());

    let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "61"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_seconds_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "1"]);
    assert!(cfg.validate().is_ok());

    let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "60"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_invalid_language_code() {
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", "en$"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_language_with_unknown_primary_code() {
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", "zz-ZZ"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_language_with_invalid_suffix_chars() {
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", "en-US$"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_language_with_region_suffixes() {
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", "en-US"]);
    assert!(cfg.validate().is_ok());
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", "pt_BR"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn accepts_auto_language() {
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", "auto"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_mic_meter_samples_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--mic-meter-ambient-ms", "100"]);
    assert!(cfg.validate().is_err());
    let mut cfg = AppConfig::parse_from(["test-app", "--mic-meter-speech-ms", "60001"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_vad_smoothing_frames_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-smoothing-frames", "0"]);
    assert!(cfg.validate().is_err());
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-smoothing-frames", "11"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_whisper_beam_size_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--whisper-beam-size", "11"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_whisper_beam_size_upper_bound() {
    let mut cfg = AppConfig::parse_from(["test-app", "--whisper-beam-size", "10"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_whisper_temperature_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--whisper-temperature=-1.0"]);
    assert!(cfg.validate().is_err());
    let mut cfg = AppConfig::parse_from(["test-app", "--whisper-temperature", "6.0"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_invalid_claude_cmd() {
    let mut cfg = AppConfig::parse_from(["test-app", "--claude-cmd", "not-claude"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn max_codex_arg_bytes_constant_matches_expectation() {
    assert_eq!(MAX_CODEX_ARG_BYTES, 8 * 1024);
}

#[test]
fn vad_engine_labels_are_stable() {
    assert_eq!(VadEngineKind::Earshot.label(), "earshot");
    assert_eq!(VadEngineKind::Simple.label(), "simple");
}

#[test]
fn accepts_valid_defaults() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn voice_vad_engine_flag_round_trips_into_pipeline_config() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-engine", "simple"]);
    cfg.validate().expect("simple VAD should be valid");
    assert!(matches!(
        cfg.voice_pipeline_config().vad_engine,
        VadEngineKind::Simple
    ));
}

#[test]
fn voice_vad_engine_default_matches_feature() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.validate().expect("defaults should be valid");
    assert_eq!(cfg.voice_vad_engine, default_vad_engine());
}

#[cfg(feature = "vad_earshot")]
#[test]
fn default_vad_engine_prefers_earshot_when_feature_enabled() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.validate().expect("defaults should be valid");
    assert!(matches!(
        cfg.voice_pipeline_config().vad_engine,
        VadEngineKind::Earshot
    ));
}

#[cfg(not(feature = "vad_earshot"))]
#[test]
fn default_vad_engine_prefers_simple_when_feature_disabled() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.validate().expect("defaults should be valid");
    assert!(matches!(
        cfg.voice_pipeline_config().vad_engine,
        VadEngineKind::Simple
    ));
}

#[test]
fn rejects_ffmpeg_device_with_shell_metacharacters() {
    for dangerous in [
        "default;rm -rf /",
        "mix|pipe",
        "out & more",
        "name$VAR",
        "quote\"",
        "single'",
        "newline\nbreak",
        "carriage\rreturn",
        "back\\slash",
    ] {
        let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", dangerous]);
        assert!(
            cfg.validate().is_err(),
            "device '{dangerous}' should be rejected"
        );
    }
}

#[cfg(unix)]
#[test]
fn codex_cmd_path_must_be_executable() {
    use std::os::unix::fs::PermissionsExt;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_path = env::temp_dir().join(format!("codex_cmd_test_{unique}"));
    fs::write(&temp_path, "#!/bin/sh\necho test\n").unwrap();
    let mut perms = fs::metadata(&temp_path).unwrap().permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&temp_path, perms.clone()).unwrap();

    let mut cfg = AppConfig::parse_from(["test-app", "--codex-cmd", temp_path.to_str().unwrap()]);
    assert!(
        cfg.validate().is_err(),
        "non-executable binary path should be rejected"
    );

    perms.set_mode(0o700);
    fs::set_permissions(&temp_path, perms).unwrap();
    let mut cfg = AppConfig::parse_from(["test-app", "--codex-cmd", temp_path.to_str().unwrap()]);
    assert!(
        cfg.validate().is_ok(),
        "executable binary path should be accepted"
    );

    let _ = fs::remove_file(&temp_path);
}

#[test]
fn rejects_invalid_voice_sample_rate() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-sample-rate", "4000"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_sample_rate_bounds() {
    let mut cfg = base_voice_config();
    cfg.voice_sample_rate = 8000;
    assert!(cfg.validate().is_ok());
    let mut cfg = base_voice_config();
    cfg.voice_sample_rate = 96000;
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_sample_rate_above_max() {
    let mut cfg = base_voice_config();
    cfg.voice_sample_rate = 96001;
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_voice_max_capture_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-max-capture-ms", "0"]);
    assert!(cfg.validate().is_err());
    let mut cfg = base_voice_config_with_capture(MAX_CAPTURE_HARD_LIMIT_MS + 1);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_max_capture_limit() {
    let mut cfg = base_voice_config_with_capture(MAX_CAPTURE_HARD_LIMIT_MS);
    assert!(cfg.validate().is_ok());
}

#[test]
fn accepts_voice_max_capture_minimum() {
    let mut cfg = base_voice_config_with_capture(200);
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_silence_tail_out_of_bounds() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_silence_tail_ms = 199;
    assert!(cfg.validate().is_err());
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_silence_tail_ms = 1001;
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_silence_tail_lower_bound() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_silence_tail_ms = 200;
    assert!(cfg.validate().is_ok());
}

#[test]
fn accepts_voice_silence_tail_equal_to_max_capture() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_silence_tail_ms = 1000;
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_min_speech_out_of_bounds() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_min_speech_ms_before_stt_start = 49;
    assert!(cfg.validate().is_err());
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_min_speech_ms_before_stt_start = 1001;
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_min_speech_lower_bound() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_min_speech_ms_before_stt_start = 50;
    assert!(cfg.validate().is_ok());
}

#[test]
fn accepts_voice_min_speech_equal_to_max_capture() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_min_speech_ms_before_stt_start = 1000;
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_lookback_exceeds_capture() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_lookback_ms = 1001;
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_lookback_equal_to_capture() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_lookback_ms = 1000;
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_buffer_smaller_than_capture_window() {
    let mut cfg = base_voice_config_with_capture(15000);
    cfg.voice_buffer_ms = 10000;
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_voice_buffer_above_max() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_buffer_ms = 120001;
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_buffer_at_bounds() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_buffer_ms = 1000;
    assert!(cfg.validate().is_ok());
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_buffer_ms = 120000;
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_channel_capacity_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "4"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_voice_channel_capacity_above_max() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "1025"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_channel_capacity_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "8"]);
    assert!(cfg.validate().is_ok());
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "1024"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_stt_timeout_out_of_bounds() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_stt_timeout_ms = 999;
    assert!(cfg.validate().is_err());
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_stt_timeout_ms = 120001;
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_stt_timeout_bounds() {
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_stt_timeout_ms = 1000;
    assert!(cfg.validate().is_ok());
    let mut cfg = base_voice_config_with_capture(1000);
    cfg.voice_stt_timeout_ms = 120000;
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_vad_threshold_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-threshold-db", "1.0"]);
    assert!(cfg.validate().is_err());
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-threshold-db=-120.1"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_vad_threshold_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-threshold-db", "0.0"]);
    assert!(cfg.validate().is_ok());
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-threshold-db=-120.0"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_voice_vad_frame_out_of_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "4"]);
    assert!(cfg.validate().is_err());
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "121"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_voice_vad_frame_bounds() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "5"]);
    assert!(cfg.validate().is_ok());
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "120"]);
    assert!(cfg.validate().is_ok());
}

#[cfg(not(feature = "vad_earshot"))]
#[test]
fn rejects_earshot_vad_engine_without_feature() {
    let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-engine", "earshot"]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_empty_language() {
    let mut cfg = AppConfig::parse_from(["test-app", "--lang", ""]);
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_too_many_codex_args() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.codex_args = (0..=MAX_CODEX_ARGS).map(|_| "x".to_string()).collect();
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_codex_args_at_limit() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.codex_args = (0..MAX_CODEX_ARGS).map(|_| "x".to_string()).collect();
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_excessive_codex_arg_bytes() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.codex_args = vec!["a".repeat(MAX_CODEX_ARG_BYTES + 1)];
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_codex_arg_bytes_at_limit() {
    let mut cfg = AppConfig::parse_from(["test-app"]);
    cfg.codex_args = vec!["a".repeat(MAX_CODEX_ARG_BYTES)];
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_ffmpeg_device_over_max_length() {
    let long_name = "a".repeat(257);
    let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", &long_name]);
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_ffmpeg_device_at_max_length() {
    let name = "a".repeat(256);
    let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", &name]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn accepts_ffmpeg_device_without_shell_chars() {
    let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", "BuiltInMic"]);
    assert!(cfg.validate().is_ok());
}

#[test]
fn voice_pipeline_config_respects_python_fallback_flag() {
    let mut cfg = AppConfig::parse_from(["test-app", "--no-python-fallback"]);
    cfg.validate().unwrap();
    assert!(!cfg.voice_pipeline_config().python_fallback_allowed);
}

#[test]
fn default_term_prefers_env() {
    static TERM_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = TERM_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let original = env::var("TERM").ok();
    env::set_var("TERM", "vt100");
    assert_eq!(default_term(), "vt100");
    env::remove_var("TERM");
    assert_eq!(default_term(), "xterm-256color");
    if let Some(value) = original {
        env::set_var("TERM", value);
    } else {
        env::remove_var("TERM");
    }
}

#[test]
fn canonical_repo_root_matches_manifest_parent() {
    let root = canonical_repo_root().unwrap();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = manifest
        .parent()
        .unwrap_or(manifest)
        .canonicalize()
        .unwrap();
    assert_eq!(root, expected);
}

fn outside_dir(repo_root: &Path, prefix: &str) -> Option<std::path::PathBuf> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let temp_root = env::temp_dir();
    let temp_root = temp_root.canonicalize().unwrap_or(temp_root);
    let temp_dir = temp_root.join(format!("{prefix}_{unique}"));
    if !temp_dir.starts_with(&repo_root) {
        return Some(temp_dir);
    }
    repo_root
        .parent()
        .map(|parent| parent.join(format!("{prefix}_{unique}")))
        .filter(|candidate| !candidate.starts_with(&repo_root))
}

#[test]
fn canonicalize_within_repo_rejects_outside_path() {
    let repo_root = canonical_repo_root().unwrap();
    let Some(outside_dir) = outside_dir(&repo_root, "voiceterm_outside") else {
        eprintln!("skipping: unable to create outside path");
        return;
    };
    let outside = outside_dir.join("outside.txt");
    if let Err(err) = fs::create_dir_all(&outside_dir) {
        eprintln!("skipping: unable to create outside path: {err}");
        return;
    }
    if let Err(err) = fs::write(&outside, "x") {
        eprintln!("skipping: unable to write outside file: {err}");
        let _ = fs::remove_dir_all(&outside_dir);
        return;
    }
    assert!(canonicalize_within_repo(&outside, "outside", &repo_root).is_err());
    let _ = fs::remove_file(outside);
    let _ = fs::remove_dir_all(outside_dir);
}

#[test]
fn canonicalize_within_repo_accepts_inside_path() {
    let repo_root = canonical_repo_root().unwrap();
    let temp_dir = repo_root.join("tmp_test_config");
    fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("inside.txt");
    fs::write(&file_path, "x").unwrap();
    let canonical = canonicalize_within_repo(&file_path, "inside", &repo_root).unwrap();
    assert!(canonical.starts_with(&repo_root));
    let _ = fs::remove_file(&file_path);
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn validate_rejects_pipeline_script_outside_repo() {
    let repo_root = canonical_repo_root().unwrap();
    let Some(outside_dir) = outside_dir(&repo_root, "pipeline_outside") else {
        eprintln!("skipping: unable to create outside path");
        return;
    };
    let script_path = outside_dir.join("pipeline.py");
    if let Err(err) = fs::create_dir_all(&outside_dir) {
        eprintln!("skipping: unable to create outside path: {err}");
        return;
    }
    let _ = fs::write(&script_path, "# test");
    let mut cfg = AppConfig::parse_from([
        "test-app",
        "--pipeline-script",
        script_path.to_str().unwrap(),
    ]);
    assert!(cfg.validate().is_err());
    let _ = fs::remove_file(&script_path);
    let _ = fs::remove_dir_all(&outside_dir);
}

#[test]
fn discover_default_whisper_model_finds_candidate() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let repo_root = env::temp_dir().join(format!("whisper_models_{unique}"));
    let models_dir = repo_root.join("whisper_models");
    fs::create_dir_all(&models_dir).unwrap();
    let candidate = models_dir.join("ggml-unit.en.bin");
    fs::write(&candidate, "x").unwrap();
    let found = discover_default_whisper_model(&repo_root, "unit");
    assert!(found.is_some());
    let _ = fs::remove_file(&candidate);
    let _ = fs::remove_dir(&models_dir);
    let _ = fs::remove_dir(&repo_root);
}

#[test]
fn discover_default_whisper_model_returns_none_when_missing() {
    let repo_root = env::temp_dir().join("whisper_models_empty");
    let _ = fs::remove_dir_all(&repo_root);
    assert!(discover_default_whisper_model(&repo_root, "unit").is_none());
}

#[test]
fn validate_rejects_missing_whisper_model_path() {
    let missing = env::temp_dir().join("missing_model.bin");
    let mut cfg = AppConfig::parse_from([
        "test-app",
        "--whisper-model-path",
        missing.to_str().unwrap(),
    ]);
    assert!(cfg.validate().is_err());
}

#[test]
fn validate_accepts_existing_whisper_model_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let model_path = env::temp_dir().join(format!("model_{unique}.bin"));
    fs::write(&model_path, "x").unwrap();
    let mut cfg = AppConfig::parse_from([
        "test-app",
        "--whisper-model-path",
        model_path.to_str().unwrap(),
    ]);
    assert!(cfg.validate().is_ok());
    let canonical = model_path.canonicalize().unwrap();
    assert_eq!(cfg.whisper_model_path.as_deref(), canonical.to_str());
    let _ = fs::remove_file(&model_path);
}

#[test]
fn sanitize_binary_accepts_allowlist_case_insensitive() {
    let sanitized = sanitize_binary("CoDeX", "--codex-cmd", &["codex"]).unwrap();
    assert_eq!(sanitized, "codex");
}

#[test]
fn sanitize_binary_rejects_empty() {
    assert!(sanitize_binary("   ", "--codex-cmd", &["codex"]).is_err());
}

#[test]
fn sanitize_binary_rejects_missing_relative_path() {
    let result = sanitize_binary("bin/does-not-exist", "--codex-cmd", &["codex"]);
    assert!(result.is_err());
}

#[test]
fn sanitize_binary_rejects_directory_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir_path = env::temp_dir().join(format!("codex_dir_{unique}"));
    fs::create_dir_all(&dir_path).unwrap();
    let result = sanitize_binary(dir_path.to_str().unwrap(), "--codex-cmd", &["codex"]);
    assert!(result.is_err());
    let _ = fs::remove_dir(&dir_path);
}

#[cfg(unix)]
#[test]
fn sanitize_binary_accepts_relative_path_with_separator() {
    use std::os::unix::fs::PermissionsExt;

    let cwd = env::current_dir().unwrap();
    let temp_dir = cwd.join("tmp_rel_bin");
    fs::create_dir_all(&temp_dir).unwrap();
    let temp_path = temp_dir.join("codex-rel");
    fs::write(&temp_path, "#!/bin/sh\n").unwrap();
    let mut perms = fs::metadata(&temp_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&temp_path, perms).unwrap();
    let rel_path = Path::new("tmp_rel_bin").join("codex-rel");
    let sanitized = sanitize_binary(rel_path.to_str().unwrap(), "--codex-cmd", &["codex"]).unwrap();
    assert!(sanitized.contains("tmp_rel_bin"));
    let _ = fs::remove_file(&temp_path);
    let _ = fs::remove_dir(&temp_dir);
}

#[cfg(unix)]
#[test]
fn sanitize_binary_accepts_executable_path() {
    use std::os::unix::fs::PermissionsExt;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_path = env::temp_dir().join(format!("codex_bin_{unique}"));
    fs::write(&temp_path, "#!/bin/sh\n").unwrap();
    let mut perms = fs::metadata(&temp_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&temp_path, perms).unwrap();
    let sanitized =
        sanitize_binary(temp_path.to_str().unwrap(), "--codex-cmd", &["codex"]).unwrap();
    assert!(sanitized.contains("codex_bin_"));
    let _ = fs::remove_file(temp_path);
}
