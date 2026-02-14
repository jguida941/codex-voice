# VoiceTerm

Voice-first terminal HUD for AI CLIs.
Talk instead of type with local Whisper transcription, then send directly to your CLI.

Primary support: Codex and Claude Code.

## Install

```bash
pipx install voiceterm
# or
python3 -m pip install --user voiceterm
```

Then run:

```bash
voiceterm
```

Authenticate your backend once if needed:

```bash
voiceterm --login --codex
voiceterm --login --claude
```

## What This Package Does

The PyPI package installs the `voiceterm` launcher.

On first run, it bootstraps the native VoiceTerm binary into:
- `~/.local/share/voiceterm/native/bin/voiceterm` (default)

By default it builds from the official VoiceTerm repository source.

## Runtime Requirements

- `git`
- Rust toolchain (`cargo`, `rustc`)
- macOS or Linux (Windows via WSL2)

## Optional Environment Overrides

- `VOICETERM_NATIVE_BIN=/absolute/path/to/voiceterm`
  - Use an already-installed native binary and skip bootstrap.
- `VOICETERM_PY_NATIVE_ROOT=/custom/root`
  - Change where the bootstrap binary is installed.
- `VOICETERM_REPO_URL=https://github.com/jguida941/voiceterm`
  - Use a different source repository URL.

## Documentation

- Main repository: https://github.com/jguida941/voiceterm
- Install guide: https://github.com/jguida941/voiceterm/blob/master/guides/INSTALL.md
- Usage guide: https://github.com/jguida941/voiceterm/blob/master/guides/USAGE.md
- CLI flags: https://github.com/jguida941/voiceterm/blob/master/guides/CLI_FLAGS.md
- Troubleshooting: https://github.com/jguida941/voiceterm/blob/master/guides/TROUBLESHOOTING.md
