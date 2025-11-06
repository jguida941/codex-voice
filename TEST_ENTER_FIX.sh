#!/bin/bash
# Test script to verify the Enter key fix

echo "=========================================="
echo "Testing Enter Key Fix for Voice Capture"
echo "=========================================="
echo ""
echo "This test verifies that:"
echo "1. First voice capture + Enter works"
echo "2. Second voice capture + Enter works (FIXED)"
echo "3. Multiple captures work consistently"
echo ""

# Clear the log
LOG_FILE="$TMPDIR/codex_voice_tui.log"
echo "Clearing log at: $LOG_FILE"
> "$LOG_FILE"

echo ""
echo "=== TEST INSTRUCTIONS ==="
echo "1. Press Ctrl+R → Speak → Press Enter"
echo "2. Wait for Codex response"
echo "3. Press Ctrl+R → Speak → Press Enter (should work now!)"
echo "4. Repeat to verify stability"
echo "5. Press Ctrl+C to exit"
echo ""
echo "If Enter key works on second+ captures, the fix is successful!"
echo ""
read -p "Press Enter to start the test..."

# Launch the fixed TUI
cd /Users/jguida941/new_github_projects/codex_voice
./scripts/run_tui.sh

echo ""
echo "=========================================="
echo "Test Complete - Analyzing Results"
echo "=========================================="
echo ""

echo "=== ENTER KEY EVENTS ==="
grep "Enter pressed" "$LOG_FILE" | tail -10 || echo "No Enter key events found"

echo ""
echo "=== VOICE CAPTURES ==="
grep "Voice capture success" "$LOG_FILE" || echo "No voice captures"

echo ""
echo "=== PROMPTS SENT ==="
grep "Attempting to send\|Prompt sent successfully" "$LOG_FILE" || echo "No prompts sent"

echo ""
echo "=== EVENT QUEUE CLEARING ==="
grep "Clear any pending events" "$LOG_FILE" 2>/dev/null || echo "(Event clearing not logged)"

echo ""
echo "=== ANY ERRORS ==="
grep -i "error\|failed" "$LOG_FILE" | grep -v "Voice capture failed" | tail -5 || echo "No errors found"

echo ""
echo "Full log saved at: $LOG_FILE"

# Summary
echo ""
echo "=========================================="
echo "SUMMARY"
echo "=========================================="
ENTER_COUNT=$(grep -c "Enter pressed" "$LOG_FILE" 2>/dev/null || echo 0)
SEND_COUNT=$(grep -c "Prompt sent successfully" "$LOG_FILE" 2>/dev/null || echo 0)
CAPTURE_COUNT=$(grep -c "Voice capture success" "$LOG_FILE" 2>/dev/null || echo 0)

echo "Enter key presses detected: $ENTER_COUNT"
echo "Successful sends: $SEND_COUNT"
echo "Voice captures: $CAPTURE_COUNT"

if [ "$SEND_COUNT" -ge 2 ]; then
    echo ""
    echo "✅ SUCCESS: Multiple sends worked! Enter key fix is successful!"
else
    echo ""
    echo "⚠️  Only $SEND_COUNT sends detected. Need at least 2 to confirm fix."
fi