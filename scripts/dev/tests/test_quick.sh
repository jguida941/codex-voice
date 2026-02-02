#!/bin/bash
echo "Testing VoxTerm TUI..."
echo "Model found: ../../models/ggml-tiny.en.bin ✓"
echo
echo "Starting TUI in background..."

# Run TUI with test input
(sleep 2; echo "v"; sleep 4; echo "q") | ../target/release/rust_tui --log-file test.log --log-timings --seconds 3 2>&1 &

PID=$!
sleep 8
kill $PID 2>/dev/null

echo
echo "=== Results ==="
if [ -f test.log ]; then
    echo "Pipeline used:"
    grep -E "(Rust pipeline|Python fallback|Native voice capture)" test.log | tail -3
    echo
    echo "Performance:"
    grep "timing|phase=voice_capture" test.log | tail -1
fi
