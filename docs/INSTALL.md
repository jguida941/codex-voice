# Installation

This doc covers all install and run options, plus model setup.

## Contents

- [Prerequisites](#prerequisites)
- [Option A: Install from source (recommended)](#option-a-install-from-source-recommended)
- [Option B: macOS App (folder picker)](#option-b-macos-app-folder-picker)
- [Option C: Homebrew (optional, global command)](#option-c-homebrew-optional-global-command)
- [Option D: Manual run (no install)](#option-d-manual-run-no-install)
- [Using with your own projects](#using-with-your-own-projects)
- [Windows](#windows)
- [See Also](#see-also)

## Prerequisites

**AI CLI (choose one):**

| CLI | Install Command |
|-----|-----------------|
| Codex (default) | `npm install -g @openai/codex` |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` |
| Gemini CLI | `npm install -g @google/gemini-cli` |

**Other requirements:**
- Rust toolchain (stable) for building from source: https://rustup.rs
- Whisper model (GGML format) - downloaded automatically on first run
- Optional (Python fallback): `python3`, `ffmpeg`, and the `whisper` CLI on PATH

## Option A: Install from source (recommended)

```bash
git clone https://github.com/jguida941/voxterm.git
cd voxterm
./install.sh
```

The installer builds the overlay, installs the `voxterm` wrapper, and downloads a Whisper
model to the correct path for the CLI.

Run from any project:

```bash
cd ~/my-project
voxterm
```

To target another AI CLI, pass `--backend` (example):

```bash
voxterm --backend claude
```

### PATH notes

If `voxterm` is not found, the installer used the first writable directory in this order:
`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, or `/path/to/voxterm/bin`.
Add that directory to PATH or set `VOXTERM_INSTALL_DIR` before running `./install.sh`.
If a `voxterm` command already exists in `/opt/homebrew/bin` or `/usr/local/bin`, the
installer skips that location to avoid clobbering system/Homebrew installs. In `~/.local/bin`
or the repo `bin/` directory it will overwrite. Remove the conflicting binary or set
`VOXTERM_INSTALL_DIR` to override.

## Option B: macOS App (folder picker)

1. Double-click **VoxTerm.app**.
2. Pick your project folder.
3. A Terminal window opens and runs the overlay inside that folder.

![Folder Picker](../img/folder-picker.png)

## Option C: Homebrew (optional, global command)

Install Homebrew (if needed):

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Tap and install:

```bash
brew tap jguida941/homebrew-voxterm
brew install voxterm
```

Run from any project (first run downloads the model if missing):

```bash
cd ~/my-project
voxterm
```

Model storage defaults to `~/.local/share/voxterm/models` for Homebrew installs (or when the
repo directory is not writable). Override with `VOXTERM_MODEL_DIR` for a custom path.

Optional pre-download:

```bash
$(brew --prefix)/opt/voxterm/libexec/scripts/setup.sh models --base
```

### Homebrew update

```bash
brew update
brew upgrade voxterm
```

If Homebrew still shows an older version (stale tap cache), force refresh:

```bash
brew untap jguida941/voxterm 2>/dev/null || true
brew untap jguida941/homebrew-voxterm 2>/dev/null || true
brew tap jguida941/homebrew-voxterm
brew update
brew info voxterm
```

If it still will not update:

```bash
rm -f "$(brew --cache)"/voxterm--*
brew reinstall voxterm
```

If you still see an older version when running `voxterm`, you likely have another
install earlier in PATH (commonly `~/.local/bin/voxterm` from `./install.sh`). Check and
remove/rename the old one:

```bash
which -a voxterm
mv ~/.local/bin/voxterm ~/.local/bin/voxterm.bak  # or delete it
hash -r
```

If you used a local install previously, also check for:

```bash
ls -l ~/voxterm/bin/voxterm 2>/dev/null
```

Then relink Homebrew and clear shell caches:

```bash
brew unlink voxterm && brew link --overwrite voxterm
hash -r
```

To verify the Homebrew binary directly (bypasses the wrapper):

```bash
$(brew --prefix)/opt/voxterm/libexec/rust_tui/target/release/voxterm --version
```

## Option D: Manual run (no install)

Run from any project folder:

```bash
VOXTERM_CWD="$(pwd)" /path/to/voxterm/start.sh
```

`start.sh` handles model download and setup when needed.

## Using with your own projects

VoxTerm works with any codebase. Run from your project directory or set `VOXTERM_CWD` to
force the working directory.

## Windows

Windows native is not supported yet (the overlay uses a Unix PTY). Use WSL2 or a macOS/Linux
machine.

## See Also

| Topic | Link |
|-------|------|
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
| Usage | [USAGE.md](USAGE.md) |
| CLI Flags | [CLI_FLAGS.md](CLI_FLAGS.md) |
| Troubleshooting | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
