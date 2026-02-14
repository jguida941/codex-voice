//! Backend command parsing so custom backend strings are split predictably.

use std::path::Path;

pub(super) fn split_backend_command(raw: &str) -> (String, Vec<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new());
    }
    let parts = shell_words::split(trimmed)
        .unwrap_or_else(|_| trimmed.split_whitespace().map(|s| s.to_string()).collect());
    if parts.is_empty() {
        return (String::new(), Vec::new());
    }
    let command = parts[0].to_string();
    let args = parts[1..].to_vec();
    (command, args)
}

pub(super) fn extract_binary_label(command: &str) -> String {
    if command.trim().is_empty() {
        return String::new();
    }
    Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command)
        .to_string()
}

pub(super) fn is_path_like(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let path = Path::new(trimmed);
    path.is_absolute() || trimmed.contains(std::path::MAIN_SEPARATOR)
}
