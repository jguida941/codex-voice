use std::process::Command;

fn combined_output(output: &std::process::Output) -> String {
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

#[test]
fn codex_overlay_help_mentions_overlay() {
    let output = Command::new(env!("CARGO_BIN_EXE_codex_overlay"))
        .arg("--help")
        .output()
        .expect("run codex_overlay --help");
    assert!(output.status.success());
    let combined = combined_output(&output);
    assert!(combined.contains("Codex Voice overlay mode"));
}

#[test]
fn codex_overlay_list_input_devices_prints_message() {
    let output = Command::new(env!("CARGO_BIN_EXE_codex_overlay"))
        .arg("--list-input-devices")
        .output()
        .expect("run codex_overlay --list-input-devices");
    assert!(output.status.success());
    let combined = combined_output(&output);
    assert!(
        combined.contains("audio input devices")
            || combined.contains("Failed to list audio input devices")
    );
}
