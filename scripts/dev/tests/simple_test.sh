#!/bin/bash
clear
echo "=========================================="
echo "     VOXTERM TUI - QUICK TEST"
echo "=========================================="
echo
echo "✓ Models available:"
ls -lh ../../models/*.bin | awk '{print "    •", $NF, "("$5")"}'
echo
echo "✓ Audio devices:"
cargo run --release -- --list-input-devices 2>&1 | sed 's/^/    /'
echo
echo "=========================================="
echo "  TO RUN THE TUI:"
echo "=========================================="
echo
echo "  cd $(pwd)"
echo "  cargo run --release -- --whisper-model-path ../../models/ggml-tiny.en.bin"
echo
echo "  Then:"
echo "    • Press Ctrl+R to start voice capture"
echo "    • Speak for 3 seconds"
echo "    • Check status line:"
echo "        - 'Rust pipeline' = FAST (native)"
echo "        - 'Python fallback' = SLOW"
echo "    • Press Ctrl+C to quit"
echo
echo "=========================================="
echo
echo "The TUI should use the RUST path since you have:"
echo "  • ggml-tiny.en.bin model ✓"
echo "  • MacBook Pro Microphone ✓"
echo "  • Release build compiled ✓"
echo
echo "If it's slow, check /tmp/voxterm_tui.log"
