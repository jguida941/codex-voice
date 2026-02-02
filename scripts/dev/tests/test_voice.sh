#!/bin/bash
# Test voice capture to see if Rust or Python path is used

export RUST_LOG=debug
export CODEX_LOG=1

LOG_FILE="${TMPDIR:-/tmp}/voxterm_tui.log"
MODEL_PATH=${MODEL_PATH:-../../models/ggml-base.en.bin}

echo "Starting TUI with debug logging..."
echo "Press Ctrl+R to start voice capture when the TUI opens"
echo "Check $LOG_FILE for whether the Rust or Python path was used"

cargo run --release -- \
  --log-timings \
  ${MODEL_PATH:+--whisper-model-path "$MODEL_PATH"} \
  "$@" 2>&1 | tee tui_output.log &
TUI_PID=$!

echo "TUI started with PID $TUI_PID"
echo "Log will be in $LOG_FILE"
echo "Press Ctrl+C to stop monitoring"

wait $TUI_PID
