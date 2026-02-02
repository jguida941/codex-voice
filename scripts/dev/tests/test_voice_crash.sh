#!/bin/bash
# Test script to verify the voice crash fix

echo "Testing voice input to Codex crash fix..."
echo "This will run the TUI with debug logging enabled"
echo ""

# Enable debug logging
export RUST_BACKTRACE=1
export CODEX_DEBUG=1

# Run with short voice capture
echo "Starting TUI with 3-second voice capture..."
echo "Say something when prompted, then watch for any crashes"
echo ""

cargo run -- --seconds 3 --lang en --codex-cmd codex

echo ""
echo "Test complete. Check debug.log for detailed output if needed."