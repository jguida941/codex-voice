//! Backend login helpers so auth flows run on the controlling TTY reliably.

/// Result type for login attempts.
pub type AuthResult = std::result::Result<(), String>;

/// Run `<command> login` using the controlling TTY so the CLI can prompt the user.
pub fn run_login_command(command: &str) -> AuthResult {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("login command is empty".to_string());
    }

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::process::{Command, Stdio};

        let tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|err| format!("failed to open /dev/tty: {err}"))?;
        let tty_in = tty
            .try_clone()
            .map_err(|err| format!("failed to clone tty for stdin: {err}"))?;
        let tty_out = tty
            .try_clone()
            .map_err(|err| format!("failed to clone tty for stdout: {err}"))?;
        let tty_err = tty;

        let status = Command::new(trimmed)
            .arg("login")
            .stdin(Stdio::from(tty_in))
            .stdout(Stdio::from(tty_out))
            .stderr(Stdio::from(tty_err))
            .status()
            .map_err(|err| format!("failed to spawn {trimmed} login: {err}"))?;

        if status.success() {
            Ok(())
        } else {
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Err(format!("login exited with code {code}"))
        }
    }

    #[cfg(not(unix))]
    {
        let _ = trimmed;
        Err("TTY auth is only supported on Unix platforms".to_string())
    }
}
