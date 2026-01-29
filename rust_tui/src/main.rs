use anyhow::Result;
use clap::Parser;
use rust_tui::{
    audio::Recorder, config::AppConfig, init_logging, ipc, log_debug, log_file_path, mic_meter, ui,
    App,
};
use std::env;

#[cfg(not(test))]
fn main() -> Result<()> {
    run_with_args(env::args_os())
}

#[cfg_attr(test, allow(dead_code))]
fn run_with_args<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut config = AppConfig::parse_from(args);

    if config.list_input_devices {
        let output = list_input_devices()?;
        print!("{output}");
        return Ok(());
    }

    if config.mic_meter {
        mic_meter::run_mic_meter(&config)?;
        return Ok(());
    }

    config.validate()?;
    init_logging(&config);
    let log_path = log_file_path();
    log_debug("=== Codex Voice TUI Started ===");
    log_debug(&format!("Log file: {log_path:?}"));

    // Run in JSON IPC mode for external UI integration
    if config.json_ipc {
        log_debug("Running in JSON IPC mode");
        return ipc::run_ipc_mode(config);
    }

    // Normal TUI mode
    let mut app = App::new(config);
    let result = ui::run_app(&mut app);

    log_debug("=== Codex Voice TUI Exiting ===");
    if let Err(ref e) = result {
        log_debug(&format!("Exit with error: {e:#}"));
    }

    result
}

fn list_input_devices() -> Result<String> {
    let devices = if let Ok(raw) = env::var("CODEX_VOICE_TEST_DEVICES") {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        }
    } else {
        Recorder::list_devices()?
    };
    let mut output = String::new();
    if devices.is_empty() {
        output.push_str("No audio input devices detected.\n");
    } else {
        output.push_str("Available audio input devices:\n");
        for name in devices {
            output.push_str(&format!("  - {name}\n"));
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    fn with_test_devices(value: Option<&str>, action: impl FnOnce() -> Result<String>) -> String {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let previous = env::var("CODEX_VOICE_TEST_DEVICES").ok();
        if let Some(value) = value {
            env::set_var("CODEX_VOICE_TEST_DEVICES", value);
        } else {
            env::remove_var("CODEX_VOICE_TEST_DEVICES");
        }

        let output = action().expect("action should succeed");

        if let Some(previous) = previous {
            env::set_var("CODEX_VOICE_TEST_DEVICES", previous);
        } else {
            env::remove_var("CODEX_VOICE_TEST_DEVICES");
        }

        output
    }

    #[test]
    fn list_input_devices_outputs_devices() {
        let output = with_test_devices(Some("Mic A,Mic B"), list_input_devices);
        assert!(output.contains("Available audio input devices:"));
        assert!(output.contains("Mic A"));
        assert!(output.contains("Mic B"));
    }

    #[test]
    fn list_input_devices_outputs_empty_message() {
        let output = with_test_devices(Some(""), list_input_devices);
        assert!(output.contains("No audio input devices detected."));
    }
}
