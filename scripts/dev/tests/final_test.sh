#!/bin/bash
echo "=== TESTING VOXTERM TUI ==="
echo
echo "Available models:"
ls -lh ../../models/*.bin 2>/dev/null | awk '{print "  ✓", $NF, "("$5")"}'
echo
echo "Starting TUI test (will run for ~8 seconds)..."
echo

# Run TUI with simulated key presses (Ctrl+R, then Ctrl+C)
(
  sleep 2
  printf '\022'   # Ctrl+R
  sleep 5      # Let it record/process
  printf '\003'   # Ctrl+C
) | cargo run --release -- --log-timings "$@" 2>&1 &

PID=$!

# Wait and kill if needed
sleep 8
kill $PID 2>/dev/null || true

# Check the debug log
echo
echo "=== CHECKING DEBUG LOG ==="
LOG_FILE="${TMPDIR:-/tmp}/voxterm_tui.log"
if [ -f "$LOG_FILE" ]; then
  echo "Recent log entries:"
  tail -20 "$LOG_FILE" | grep -E "(capture_voice|Recording|Transcr|pipeline|fallback)" | tail -10
  echo
  
  # Check which path was used
  if grep -q "capture_voice_native" "$LOG_FILE"; then
    echo "✓ USING RUST NATIVE PATH (fast)"
    grep "timing|phase=voice_capture" "$LOG_FILE" | tail -1
  elif grep -q "Python fallback" "$LOG_FILE"; then
    echo "⚠ USING PYTHON FALLBACK (slow)"
  else
    echo "Status unclear, showing all recent logs:"
    tail -5 "$LOG_FILE"
  fi
else
  echo "No debug log found at $LOG_FILE"
  echo "The TUI may not be logging properly"
fi

echo
echo "To run manually:"
echo "  cd $(pwd)"
echo "  cargo run --release -- --whisper-model-path ../../models/ggml-base.en.bin"
echo "Then press Ctrl+R for voice and Ctrl+C to quit."
