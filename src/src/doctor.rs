//! Doctor-report assembly that surfaces runtime diagnostics and environment mismatches.

use crate::{audio::Recorder, config::AppConfig, crash_log_path, log_file_path};
use crossterm::terminal::size as terminal_size;
use std::{env, fmt::Display};

/// Structured text report builder used by `--doctor` commands.
pub struct DoctorReport {
    lines: Vec<String>,
}

impl DoctorReport {
    /// Create a new report with the provided title line.
    pub fn new(title: &str) -> Self {
        Self {
            lines: vec![title.to_string()],
        }
    }

    /// Append a section heading and blank separator line.
    pub fn section(&mut self, title: &str) {
        self.lines.push(String::new());
        self.lines.push(format!("{title}:"));
    }

    /// Append a `key: value` line in doctor output format.
    pub fn push_kv(&mut self, key: &str, value: impl Display) {
        self.lines.push(format!("  {key}: {value}"));
    }

    /// Append a raw line without key/value formatting.
    pub fn push_line(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    /// Render the full report as newline-separated text.
    pub fn render(&self) -> String {
        self.lines.join("\n")
    }
}

/// Build the baseline doctor report shared by all VoiceTerm binaries.
pub fn base_doctor_report(config: &AppConfig, binary_name: &str) -> DoctorReport {
    let mut report = DoctorReport::new("VoiceTerm Doctor");
    report.push_kv("version", env!("CARGO_PKG_VERSION"));
    report.push_kv("binary", binary_name);
    report.push_kv("os", format!("{}/{}", env::consts::OS, env::consts::ARCH));

    let mut validated = config.clone();
    let validation_result = validated.validate();
    let resolved = validation_result
        .as_ref()
        .map(|_| &validated)
        .unwrap_or(config);

    report.section("Terminal");
    match terminal_size() {
        Ok((cols, rows)) => report.push_kv("size", format!("{cols}x{rows}")),
        Err(err) => report.push_kv("size", format!("error: {err}")),
    }
    if let Ok(term) = env::var("TERM") {
        report.push_kv("term", term);
    }
    if let Ok(colorterm) = env::var("COLORTERM") {
        report.push_kv("colorterm", colorterm);
    }
    if let Some(term_program) = format_term_program_for_report() {
        report.push_kv("term_program", term_program);
    }
    if env::var("NO_COLOR").is_ok() {
        report.push_kv("no_color", "set");
    }
    report.push_kv("color_mode", detect_color_mode());
    report.push_kv("unicode", detect_unicode_support());
    report.push_kv("graphics", detect_graphics_protocol());
    report.push_kv("mouse_capture", "disabled (not enabled by app)");

    report.section("Config");
    match validation_result {
        Ok(()) => report.push_kv("validation", "ok"),
        Err(err) => report.push_kv("validation", format!("error: {err}")),
    }
    let logs_enabled = (resolved.logs || resolved.log_timings) && !resolved.no_logs;
    report.push_kv("logs", if logs_enabled { "enabled" } else { "disabled" });
    report.push_kv(
        "log_content",
        if resolved.log_content {
            "enabled"
        } else {
            "disabled"
        },
    );
    report.push_kv("log_file", log_file_path().display());
    report.push_kv("crash_log", crash_log_path().display());
    report.push_kv("pipeline_script", resolved.pipeline_script.display());
    report.push_kv("whisper_model", &resolved.whisper_model);
    report.push_kv(
        "whisper_model_path",
        resolved.whisper_model_path.as_deref().unwrap_or("unset"),
    );
    report.push_kv("python_cmd", &resolved.python_cmd);
    report.push_kv("ffmpeg_cmd", &resolved.ffmpeg_cmd);

    report.section("Audio");
    report.push_kv(
        "input_device",
        resolved.input_device.as_deref().unwrap_or("default"),
    );
    match Recorder::list_devices() {
        Ok(devices) => {
            report.push_kv("device_count", devices.len());
            if devices.is_empty() {
                report.push_kv("devices", "none");
            } else {
                report.push_line("  devices:");
                for name in devices {
                    report.push_line(format!("    - {name}"));
                }
            }
        }
        Err(err) => report.push_kv("devices", format!("error: {err}")),
    }

    report
}

fn has_cursor_marker_env() -> bool {
    for key in [
        "CURSOR_TRACE_ID",
        "CURSOR_APP_VERSION",
        "CURSOR_VERSION",
        "CURSOR_BUILD_VERSION",
    ] {
        if env::var(key)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn cursor_version_env() -> Option<String> {
    for key in [
        "CURSOR_APP_VERSION",
        "CURSOR_VERSION",
        "CURSOR_BUILD_VERSION",
    ] {
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn format_term_program_for_report() -> Option<String> {
    let term_program = env::var("TERM_PROGRAM").ok()?;
    let term_program_version = env::var("TERM_PROGRAM_VERSION").ok();
    let is_cursor = term_program.eq_ignore_ascii_case("cursor") || has_cursor_marker_env();

    if is_cursor {
        if term_program.eq_ignore_ascii_case("cursor") {
            let version = cursor_version_env()
                .or(term_program_version)
                .unwrap_or_else(|| "unknown".to_string());
            return Some(format!("cursor ({version})"));
        }

        let cursor_version = cursor_version_env();
        return Some(match (cursor_version, term_program_version) {
            (Some(cursor_version), Some(engine_version)) => format!(
                "cursor ({cursor_version}; terminal engine {term_program} {engine_version})"
            ),
            (Some(cursor_version), None) => {
                format!("cursor ({cursor_version}; terminal engine {term_program} unknown)")
            }
            (None, Some(engine_version)) => {
                format!("cursor (terminal engine {term_program} {engine_version})")
            }
            (None, None) => format!("cursor (terminal engine {term_program} unknown)"),
        });
    }

    Some(format!(
        "{term_program} ({})",
        term_program_version.unwrap_or_else(|| "unknown".to_string())
    ))
}

fn detect_color_mode() -> String {
    if env::var("NO_COLOR").is_ok() {
        return "none (NO_COLOR)".to_string();
    }
    if let Ok(colorterm) = env::var("COLORTERM") {
        let value = colorterm.to_lowercase();
        if value == "truecolor" || value == "24bit" {
            return format!("truecolor (COLORTERM={colorterm})");
        }
    }
    if let Ok(term) = env::var("TERM") {
        let value = term.to_lowercase();
        if value.contains("256color") || value.contains("256-color") {
            return format!("256 (TERM={term})");
        }
        if value.contains("color") || value.contains("xterm") || value.contains("screen") {
            return format!("ansi (TERM={term})");
        }
        if value == "dumb" {
            return "none (TERM=dumb)".to_string();
        }
    }
    "ansi (default)".to_string()
}

fn detect_unicode_support() -> String {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(value) = env::var(key) {
            let upper = value.to_ascii_uppercase();
            if upper.contains("UTF-8") || upper.contains("UTF8") {
                return format!("likely ({key}={value})");
            }
            return format!("unknown ({key}={value})");
        }
    }
    "unknown (locale env not set)".to_string()
}

fn detect_graphics_protocol() -> String {
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return "kitty".to_string();
    }
    if env::var("WEZTERM_PANE").is_ok() || env::var("WEZTERM_EXECUTABLE").is_ok() {
        return "wezterm".to_string();
    }
    if let Ok(term_program) = env::var("TERM_PROGRAM") {
        if term_program == "iTerm.app" {
            return "iterm2".to_string();
        }
        if term_program == "Apple_Terminal" {
            return "apple terminal".to_string();
        }
    }
    if env::var("VTE_VERSION").is_ok() {
        return "vte".to_string();
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        static ENV_GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        f()
    }

    fn set_or_clear_env(key: &str, value: Option<&str>) {
        match value {
            Some(v) => env::set_var(key, v),
            None => env::remove_var(key),
        }
    }

    fn with_term_program_env<T>(
        term_program: Option<&str>,
        term_program_version: Option<&str>,
        cursor_trace_id: Option<&str>,
        cursor_app_version: Option<&str>,
        f: impl FnOnce() -> T,
    ) -> T {
        with_env_lock(|| {
            let keys = [
                "TERM_PROGRAM",
                "TERM_PROGRAM_VERSION",
                "CURSOR_TRACE_ID",
                "CURSOR_APP_VERSION",
            ];
            let prev: Vec<(String, Option<String>)> = keys
                .iter()
                .map(|key| ((*key).to_string(), env::var(key).ok()))
                .collect();

            set_or_clear_env("TERM_PROGRAM", term_program);
            set_or_clear_env("TERM_PROGRAM_VERSION", term_program_version);
            set_or_clear_env("CURSOR_TRACE_ID", cursor_trace_id);
            set_or_clear_env("CURSOR_APP_VERSION", cursor_app_version);

            let result = f();

            for (key, value) in prev {
                set_or_clear_env(&key, value.as_deref());
            }
            result
        })
    }

    #[test]
    fn format_term_program_for_report_defaults_to_raw_term_program() {
        with_term_program_env(Some("vscode"), Some("1.97.0"), None, None, || {
            assert_eq!(
                format_term_program_for_report(),
                Some("vscode (1.97.0)".to_string())
            );
        });
    }

    #[test]
    fn format_term_program_for_report_labels_cursor_when_markers_present() {
        with_term_program_env(Some("vscode"), Some("1.97.0"), Some("trace"), None, || {
            assert_eq!(
                format_term_program_for_report(),
                Some("cursor (terminal engine vscode 1.97.0)".to_string())
            );
        });
    }

    #[test]
    fn format_term_program_for_report_prefers_cursor_version_when_available() {
        with_term_program_env(
            Some("vscode"),
            Some("1.97.0"),
            Some("trace"),
            Some("0.46.0"),
            || {
                assert_eq!(
                    format_term_program_for_report(),
                    Some("cursor (0.46.0; terminal engine vscode 1.97.0)".to_string())
                );
            },
        );
    }

    #[test]
    fn format_term_program_for_report_handles_native_cursor_term_program() {
        with_term_program_env(Some("cursor"), Some("0.47.0"), None, None, || {
            assert_eq!(
                format_term_program_for_report(),
                Some("cursor (0.47.0)".to_string())
            );
        });
    }
}
