#!/bin/bash
# Minimal test to check if panic still happens

echo "Testing with problematic input..."
echo "│> Testing. 😊 你好 0;0;0u" | ../target/release/rust_tui --seconds 1 --no-persistent-codex 2>&1 | grep -E "(panic|thread|index|byte)" || echo "No panic detected"

echo ""
echo "Testing with long input..."
python3 -c "print('a' * 499 + '世界')" | ../target/release/rust_tui --seconds 1 --no-persistent-codex 2>&1 | grep -E "(panic|thread|index|byte)" || echo "No panic detected"