#!/bin/bash
# Test script for the fixed voice capture in Rust TUI

set -euo pipefail

echo "========================================="
echo "Codex Voice TUI - Voice Capture Test"
echo "========================================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Determine log file location
if [[ "$OSTYPE" == "darwin"* ]]; then
    LOG_FILE="$TMPDIR/codex_voice_tui.log"
else
    LOG_FILE="/tmp/codex_voice_tui.log"
fi

echo -e "${BLUE}Debug Log Location:${NC} $LOG_FILE"
echo ""

# Clear previous log if it exists
if [ -f "$LOG_FILE" ]; then
    echo -e "${YELLOW}Clearing previous log file...${NC}"
    > "$LOG_FILE"
fi

echo -e "${GREEN}=== FIX APPLIED ===${NC}"
echo ""
echo "The TUI now uses a terminal state wrapper that:"
echo "  1. Exits TUI mode before running ffmpeg/whisper"
echo "  2. Runs commands in normal terminal mode"
echo "  3. Returns to TUI mode after completion"
echo ""
echo -e "${YELLOW}What to expect:${NC}"
echo "  • Screen may flicker when pressing Ctrl+R (mode switching)"
echo "  • Voice recording should work without crashing"
echo "  • Transcript should appear in the input field"
echo "  • TUI should remain functional after voice capture"
echo ""
echo -e "${GREEN}Testing Instructions:${NC}"
echo "  1. Press Ctrl+R to record voice"
echo "  2. Speak for a few seconds"
echo "  3. Wait for transcription to complete"
echo "  4. Verify transcript appears in input field"
echo "  5. Press Ctrl+C to exit"
echo ""
echo -e "${BLUE}Monitoring the log:${NC}"
echo "  In another terminal, run:"
echo "  tail -f $LOG_FILE"
echo ""
read -p "Press Enter to launch the TUI..."

# Launch the TUI
echo -e "${GREEN}Launching TUI...${NC}"
./scripts/run_tui.sh

# After TUI exits, show log summary
echo ""
echo -e "${YELLOW}=== Session Summary ===${NC}"
if [ -f "$LOG_FILE" ]; then
    echo "Log entries:"
    echo "------------"
    tail -20 "$LOG_FILE"
else
    echo "No log file found"
fi

echo ""
echo -e "${BLUE}Full log available at:${NC} $LOG_FILE"