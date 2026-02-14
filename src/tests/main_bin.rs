//! Integration tests that lock main-binary startup behavior and smoke paths.

use std::process::Command;

#[test]
fn main_lists_input_devices() {
    let bin = env!("CARGO_BIN_EXE_voiceterm");
    let output = Command::new(bin)
        .arg("--list-input-devices")
        .env("VOICETERM_TEST_DEVICES", "Mic A,Mic B")
        .output()
        .expect("run voiceterm");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Available audio input devices:"));
    assert!(stdout.contains("Mic A"));
    assert!(stdout.contains("Mic B"));
}

#[test]
fn main_reports_no_input_devices() {
    let bin = env!("CARGO_BIN_EXE_voiceterm");
    let output = Command::new(bin)
        .arg("--list-input-devices")
        .env("VOICETERM_TEST_DEVICES", "")
        .output()
        .expect("run voiceterm");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No audio input devices detected."));
}
