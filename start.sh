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

# Choose overlay (default) or legacy TypeScript CLI
MODE="${CODEX_VOICE_MODE:-overlay}"

# Check if Rust binary exists
if [ "$MODE" = "overlay" ]; then
    if [ ! -f "rust_tui/target/release/codex_overlay" ]; then
        echo -e "${YELLOW}Building Rust overlay (first time setup)...${NC}"
        cd rust_tui && cargo build --release --bin codex_overlay
        if [ $? -ne 0 ]; then
            echo -e "${RED}Build failed. Please check the error above.${NC}"
            exit 1
        fi
        cd ..
    fi
else
    if [ ! -f "rust_tui/target/release/rust_tui" ]; then
        echo -e "${YELLOW}Building Rust backend (first time setup)...${NC}"
        cd rust_tui && cargo build --release
        if [ $? -ne 0 ]; then
            echo -e "${RED}Build failed. Please check the error above.${NC}"
            exit 1
        fi
        cd ..
    fi
fi

# Check if whisper model exists
MODEL_PATH=""
HAS_WHISPER_ARG=0
for arg in "$@"; do
    case "$arg" in
        --whisper-model-path|--whisper-model-path=*)
            HAS_WHISPER_ARG=1
            ;;
    esac
done

find_model() {
    for candidate in \
        "models/ggml-small.en.bin" \
        "models/ggml-small.bin" \
        "models/ggml-base.en.bin" \
        "models/ggml-base.bin" \
        "models/ggml-tiny.en.bin" \
        "models/ggml-tiny.bin"; do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

MODEL_PATH="$(find_model || true)"
if [ -z "$MODEL_PATH" ]; then
    echo -e "${YELLOW}Downloading Whisper model (first time setup)...${NC}"
    ./scripts/setup.sh models --base
    if [ $? -ne 0 ]; then
        echo -e "${RED}Model download failed. Please check the error above.${NC}"
        exit 1
    fi
    MODEL_PATH="$(find_model || true)"
fi

if [ -z "$MODEL_PATH" ]; then
    echo -e "${RED}Whisper model not found in ./models. Run: ./scripts/setup.sh models --base${NC}"
    exit 1
fi

if [ "$MODE" = "overlay" ]; then
    echo -e "${GREEN}Launching overlay mode...${NC}"
    cd rust_tui
    EXTRA_ARGS=()
    if [ $HAS_WHISPER_ARG -eq 0 ]; then
        EXTRA_ARGS+=(--whisper-model-path "../$MODEL_PATH")
    fi
    ./target/release/codex_overlay "${EXTRA_ARGS[@]}" "$@"
else
    # Check if TypeScript is built
    if [ ! -f "ts_cli/dist/index.js" ]; then
        echo -e "${YELLOW}Building TypeScript CLI...${NC}"
        cd ts_cli && npm install && npm run build
        cd ..
    fi

    # Run the CLI
    cd ts_cli
    npm start
fi
