#!/bin/bash
# Test performance of voice capture

echo "=== VoxTerm TUI Performance Test ==="
echo

# Check if model exists (default tiny)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODEL_PATH=${MODEL_PATH:-"$SCRIPT_DIR/../../models/ggml-tiny.en.bin"}
if [ ! -f "$MODEL_PATH" ]; then
    echo "⚠️  No Whisper model found. Will use Python fallback (slower)."
    echo "   Download a model with:"
    echo "   curl -L 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin' -o $MODEL_PATH"
    echo
fi

# Build in release mode for best performance
echo "Building release binary..."
cargo build --release --features high-quality-audio

echo
LOG_FILE="${TMPDIR:-/tmp}/voxterm_tui.log"

echo "Running TUI with performance logging..."
echo "Press Ctrl+R to test voice capture, Ctrl+C to quit"
echo

# Run with timing logs (explicit model path if available)
cargo run --release -- \
    --log-timings \
    --seconds 3 \
    ${MODEL_PATH:+--whisper-model-path "$MODEL_PATH"} \
    "$@"

echo
echo "=== Performance Summary ==="
if [ -f "$LOG_FILE" ]; then
    echo "Voice capture timings:"
    grep "timing|phase=voice_capture" "$LOG_FILE" | tail -5
    echo
    echo "Pipeline used:"
    grep -E "(Rust pipeline|Python fallback)" "$LOG_FILE" | tail -5
else
    echo "No log file found at $LOG_FILE"
fi
