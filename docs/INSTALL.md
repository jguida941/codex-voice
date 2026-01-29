# Installation

This doc covers all install and run options, plus model setup.

## Prerequisites

- Codex CLI: `npm install -g @openai/codex`
- Rust toolchain (stable) for building from source: https://rustup.rs
- Whisper model (GGML format). The install and start scripts can download one automatically.

## Option A: Install from source (recommended)

```bash
git clone https://github.com/jguida941/codex-voice.git
cd codex-voice
./install.sh
```

The installer builds the overlay, installs the `codex-voice` wrapper, and downloads a Whisper
model to the correct path for the CLI.

Run from any project:

```bash
cd ~/my-project
codex-voice
```

### PATH notes

If `codex-voice` is not found, the installer used the first writable directory in this order:
`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, or `/path/to/codex-voice/bin`.
Add that directory to PATH or set `CODEX_VOICE_INSTALL_DIR` before running `./install.sh`.
If a `codex-voice` command already exists, the installer skips that location; remove the
conflicting binary or set `CODEX_VOICE_INSTALL_DIR` to override.

## Option B: macOS App (folder picker)

1. Double-click **Codex Voice.app**.
2. Pick your project folder.
3. A Terminal window opens and runs the overlay inside that folder.

## Option C: Homebrew (optional, global command)

Install Homebrew (if needed):

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Tap and install:

```bash
brew tap jguida941/homebrew-codex-voice
brew install codex-voice
```

Run from any project (first run downloads the model if missing):

```bash
cd ~/my-project
codex-voice
```

Model storage defaults to `~/.local/share/codex-voice/models` for Homebrew installs (or when the
repo directory is not writable). Override with `CODEX_VOICE_MODEL_DIR` for a custom path.

Optional pre-download:

```bash
$(brew --prefix)/opt/codex-voice/libexec/scripts/setup.sh models --base
```

### Homebrew update

```bash
brew update
brew upgrade codex-voice
```

If Homebrew still shows an older version (stale tap cache), force refresh:

```bash
brew untap jguida941/codex-voice 2>/dev/null || true
brew untap jguida941/homebrew-codex-voice 2>/dev/null || true
brew tap jguida941/homebrew-codex-voice
brew update
brew info codex-voice
```

If it still will not update:

```bash
rm -f "$(brew --cache)"/codex-voice--*
brew reinstall codex-voice
```

If you still see an older version when running `codex-voice`, you likely have another
install earlier in PATH (commonly `~/.local/bin/codex-voice` from `./install.sh`). Check and
remove/rename the old one:

```bash
which -a codex-voice
mv ~/.local/bin/codex-voice ~/.local/bin/codex-voice.bak  # or delete it
hash -r
```

If you used a local install previously, also check for:

```bash
ls -l ~/codex-voice/bin/codex-voice 2>/dev/null
```

Then relink Homebrew and clear shell caches:

```bash
brew unlink codex-voice && brew link --overwrite codex-voice
hash -r
```

To verify the Homebrew binary directly (bypasses the wrapper):

```bash
$(brew --prefix)/opt/codex-voice/libexec/rust_tui/target/release/codex-voice --version
```

## Option D: Manual run (no install)

Run from any project folder:

```bash
CODEX_VOICE_CWD="$(pwd)" /path/to/codex-voice/start.sh
```

`start.sh` handles model download and setup when needed.

## Using with your own projects

Codex Voice works with any codebase. Run from your project directory or set `CODEX_VOICE_CWD` to
force the working directory.

## Windows

Windows native is not supported yet (the overlay uses a Unix PTY). Use WSL2 or a macOS/Linux
machine.
