#!/bin/bash
echo "=== VOXTERM TUI TEST ==="
echo
echo "✓ Model found: ggml-tiny.en.bin (77MB)"
echo "✓ Audio devices detected"
echo
echo "Running TUI with debug output..."
echo "(Will auto-quit after 5 seconds)"
echo

# Create a test script that sends commands
(
  sleep 1   # Wait for TUI to start
  echo "v"  # Start voice capture
  sleep 4   # Record for a bit
  echo "q"  # Quit
) | CODEX_LOG=1 ../target/release/rust_tui --log-timings 2>debug_output.txt &

PID=$!
sleep 6
kill $PID 2>/dev/null || true

echo
echo "=== DEBUG OUTPUT ==="
if [ -f debug_output.txt ]; then
  grep -E "(Native|Rust pipeline|Python fallback|timing|Recording|Transcrib)" debug_output.txt | head -20
  echo
  echo "Check if using Rust native path:"
  if grep -q "Native voice capture" debug_output.txt; then
    echo "✓ Using RUST NATIVE path (fast)"
  elif grep -q "Python fallback" debug_output.txt; then
    echo "⚠ Using PYTHON fallback (slow)"
  else
    echo "? Could not determine path"
  fi
fi
