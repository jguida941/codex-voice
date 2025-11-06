#!/bin/bash
# Quick test script to reproduce the Enter key issue

echo "======================================"
echo "Codex Voice TUI - Quick Issue Test"
echo "======================================"
echo ""
echo "This test will help debug why the Enter key"
echo "doesn't work after the second voice capture."
echo ""

# Clear log
LOG_FILE="$TMPDIR/codex_voice_tui.log"
echo "Clearing log at: $LOG_FILE"
> "$LOG_FILE"

echo ""
echo "INSTRUCTIONS:"
echo "1. Press Ctrl+R → Speak → Press Enter (should work)"
echo "2. Press Ctrl+R → Speak → Press Enter (will fail)"
echo "3. When Enter fails, try:"
echo "   - Press 'a' (does it type?)"
echo "   - Press Backspace (does it delete?)"
echo "   - Press F2 (does anything happen?)"
echo "4. Press Ctrl+C to exit"
echo ""
read -p "Press Enter to launch TUI..."

# Launch TUI
cd /Users/jguida941/new_github_projects/codex_voice
./scripts/run_tui.sh

echo ""
echo "======================================"
echo "Test Complete - Analyzing Log"
echo "======================================"
echo ""

echo "=== ENTER KEY PRESSES ==="
grep "Enter pressed" "$LOG_FILE" || echo "No Enter key presses detected"

echo ""
echo "=== VOICE CAPTURES ==="
grep "Voice capture success" "$LOG_FILE" || echo "No voice captures logged"

echo ""
echo "=== INPUT FIELD STATE ==="
grep "Input field now contains" "$LOG_FILE" || echo "No input field state logged"

echo ""
echo "=== KEY EVENTS (last 10) ==="
grep "Key event" "$LOG_FILE" | tail -10 || echo "No key events logged"

echo ""
echo "=== ERRORS ==="
grep -i "error\|failed" "$LOG_FILE" || echo "No errors found"

echo ""
echo "Full log saved at: $LOG_FILE"
echo "To view full log: cat $LOG_FILE"