#!/bin/bash
#
# Codex Voice - Quick Start
# Double-click this file or run: ./start.sh
#

# Save the user's current directory so codex-voice works on their project
export CODEX_VOICE_CWD="$(pwd)"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo ""
echo -e "${GREEN}Starting Codex Voice...${NC}"
echo ""

# Check if Rust binary exists
if [ ! -f "rust_tui/target/release/rust_tui" ]; then
    echo -e "${YELLOW}Building Rust backend (first time setup)...${NC}"
    cd rust_tui && cargo build --release
    if [ $? -ne 0 ]; then
        echo -e "${RED}Build failed. Please check the error above.${NC}"
        exit 1
    fi
    cd ..
fi

# Check if whisper model exists
if [ ! -f "models/ggml-base.en.bin" ] && [ ! -f "models/ggml-tiny.en.bin" ]; then
    echo -e "${YELLOW}Downloading Whisper model (first time setup)...${NC}"
    ./scripts/setup.sh models --base
fi

# Check if TypeScript is built
if [ ! -f "ts_cli/dist/index.js" ]; then
    echo -e "${YELLOW}Building TypeScript CLI...${NC}"
    cd ts_cli && npm install && npm run build
    cd ..
fi

# Run the CLI
cd ts_cli
npm start
