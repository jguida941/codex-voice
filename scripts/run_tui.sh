#!/usr/bin/env bash
# Wrapper to launch the Rust TUI with sane defaults so Codex runs inside IDEs.

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="${ROOT_DIR}/rust_tui"

# Activate venv if it exists
if [ -f "${ROOT_DIR}/.venv/bin/activate" ]; then
    echo "Activating Python virtual environment..."
    source "${ROOT_DIR}/.venv/bin/activate"
fi

# Determine which whisper command to use
if command -v whisper &> /dev/null; then
    # Use real whisper if available
    WHISPER_CMD="${ROOT_DIR}/.venv/bin/whisper"
    if [ ! -f "$WHISPER_CMD" ]; then
        WHISPER_CMD="whisper"
    fi
    echo "Using whisper: $WHISPER_CMD"
else
    # Fall back to fake whisper stub for testing
    WHISPER_CMD="${ROOT_DIR}/stubs/fake_whisper"
    echo "WARNING: Using fake_whisper stub (real whisper not found)"
fi

SECONDS_ARG="${SECONDS_OVERRIDE:-5}"
FFMPEG_DEVICE_ARG="${FFMPEG_DEVICE_OVERRIDE:-:0}"
WHISPER_MODEL_ARG="${WHISPER_MODEL_OVERRIDE:-base}"
CODEX_CMD_ARG="${CODEX_CMD_OVERRIDE:-codex}"
TERM_VALUE="${TERM_OVERRIDE:-xterm-256color}"
PYTHON_CMD_ARG="${PYTHON_CMD_OVERRIDE:-python3}"

cd "$PROJECT_DIR"

exec cargo run -- \
  --seconds "$SECONDS_ARG" \
  --ffmpeg-device "$FFMPEG_DEVICE_ARG" \
  --whisper-cmd "$WHISPER_CMD" \
  --whisper-model "$WHISPER_MODEL_ARG" \
  --codex-cmd "$CODEX_CMD_ARG" \
  --term "$TERM_VALUE" \
  --python-cmd "$PYTHON_CMD_ARG"
