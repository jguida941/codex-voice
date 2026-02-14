# VoiceTerm PyPI Package

This package installs a `voiceterm` launcher for Python environments.

On first run, the launcher bootstraps the native Rust CLI into:

- `~/.local/share/voiceterm/native/bin/voiceterm` (default)

It uses:

- `git clone` of the VoiceTerm repo
- `cargo install --path src --bin voiceterm`

## Runtime requirements

- `git`
- Rust toolchain (`cargo`, `rustc`)

If you already have the native binary elsewhere, set:

- `VOICETERM_NATIVE_BIN=/absolute/path/to/voiceterm`

To override the bootstrap install root:

- `VOICETERM_PY_NATIVE_ROOT=/custom/root`

To override the source repo URL:

- `VOICETERM_REPO_URL=https://github.com/jguida941/voiceterm`

