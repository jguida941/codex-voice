//! Integration tests that lock voiceterm CLI flag and output behavior.

use std::process::Command;

fn combined_output(output: &std::process::Output) -> String {
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

fn voiceterm_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_voiceterm").expect("voiceterm test binary not built")
}

#[test]
fn voiceterm_help_mentions_name() {
    let output = Command::new(voiceterm_bin())
        .arg("--help")
        .output()
        .expect("run voiceterm --help");
    assert!(output.status.success());
    let combined = combined_output(&output);
    assert!(combined.contains("VoiceTerm"));
}

#[test]
fn voiceterm_list_input_devices_prints_message() {
    let output = Command::new(voiceterm_bin())
        .arg("--list-input-devices")
        .output()
        .expect("run voiceterm --list-input-devices");
    assert!(output.status.success());
    let combined = combined_output(&output);
    assert!(
        combined.contains("audio input devices")
            || combined.contains("Failed to list audio input devices")
    );
}
