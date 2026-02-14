//! CLI startup utilities so boot-time checks stay consistent across entry paths.

use anyhow::Result;
use voiceterm::audio;

pub(crate) fn resolve_sound_flag(global: bool, specific: bool) -> bool {
    global || specific
}

pub(crate) fn should_print_stats(stats_output: &str) -> bool {
    !stats_output.is_empty()
}

pub(crate) fn list_input_devices() -> Result<()> {
    // Support VOICETERM_TEST_DEVICES for testing
    let devices = if let Ok(raw) = std::env::var("VOICETERM_TEST_DEVICES") {
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
        audio::Recorder::list_devices().unwrap_or_else(|err| {
            eprintln!("Failed to list audio input devices: {err}");
            Vec::new()
        })
    };

    if devices.is_empty() {
        println!("No audio input devices detected.");
    } else {
        println!("Available audio input devices:");
        for name in devices {
            println!("  - {name}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_sound_flag_prefers_global() {
        assert!(!resolve_sound_flag(false, false));
        assert!(resolve_sound_flag(true, false));
        assert!(resolve_sound_flag(false, true));
        assert!(resolve_sound_flag(true, true));
    }

    #[test]
    fn should_print_stats_requires_non_empty() {
        assert!(!should_print_stats(""));
        assert!(should_print_stats("stats"));
    }
}
