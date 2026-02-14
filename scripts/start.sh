#!/bin/bash
#
# VoiceTerm - Quick Start
# Double-click this file or run: ./scripts/start.sh (from repo root)
#

# Detect platform
OS="$(uname -s)"
case "$OS" in
    Darwin) PLATFORM="macos" ;;
    Linux)  PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *)      PLATFORM="unknown" ;;
esac

if [ "$PLATFORM" = "windows" ]; then
    echo "Windows is not yet supported. Try WSL2 with Linux instructions."
    exit 1
fi

# Save the user's current directory so voiceterm works on their project
export VOICETERM_CWD="$(pwd)"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SCRIPT_DIR"

# Colors - Vibrant red theme
CORAL='\033[38;2;255;90;90m'
CORAL_BRIGHT='\033[38;2;255;110;110m'
GREEN='\033[0;32m'
GOLD='\033[38;5;214m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
DIM='\033[2m'
BOLD='\033[1m'
NC='\033[0m'

if [ -n "${NO_COLOR:-}" ]; then
    CORAL=''
    CORAL_BRIGHT=''
    GREEN=''
    GOLD=''
    YELLOW=''
    RED=''
    DIM=''
    BOLD=''
    NC=''
fi

TERM_COLS="${VOICETERM_FORCE_COLUMNS:-${COLUMNS:-$(tput cols 2>/dev/null || true)}}"
if ! [ "$TERM_COLS" -gt 0 ] 2>/dev/null; then
    TERM_COLS=80
fi
TERM_LINES="${VOICETERM_FORCE_LINES:-${LINES:-$(tput lines 2>/dev/null || true)}}"
if ! [ "$TERM_LINES" -gt 0 ] 2>/dev/null; then
    TERM_LINES=24
fi

# Get version from Cargo.toml
VERSION="1.0.33"
if [ -f "$SCRIPT_DIR/src/Cargo.toml" ]; then
    VERSION=$(grep '^version' "$SCRIPT_DIR/src/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
fi

BACKEND_LABEL="codex"
THEME_LABEL="coral"
AUTO_LABEL="off"
if [ -n "${NO_COLOR:-}" ]; then
    THEME_LABEL="none"
fi

ARGS=("$@")
i=0
while [ "$i" -lt "${#ARGS[@]}" ]; do
    arg="${ARGS[$i]}"
    case "$arg" in
        --backend)
            i=$((i + 1))
            BACKEND_LABEL="${ARGS[$i]:-codex}"
            ;;
        --backend=*)
            BACKEND_LABEL="${arg#*=}"
            ;;
        --theme)
            i=$((i + 1))
            THEME_LABEL="${ARGS[$i]:-coral}"
            ;;
        --theme=*)
            THEME_LABEL="${arg#*=}"
            ;;
        --no-color|--no-colour)
            THEME_LABEL="none"
            ;;
        --auto-voice)
            AUTO_LABEL="on"
            ;;
    esac
    i=$((i + 1))
done

if [[ "$BACKEND_LABEL" == *" "* ]]; then
    BACKEND_LABEL="custom"
fi
BACKEND_LABEL="$(basename "$BACKEND_LABEL")"

# Minimal startup - Rust binary shows the full banner
echo ""

# Startup output-only mode for tests
if [ "${VOICETERM_STARTUP_ONLY:-0}" = "1" ]; then
    exit 0
fi

# Resolve binary (prefer local build; avoid wrapper recursion)
OVERLAY_BIN=""
if [ -x "$SCRIPT_DIR/src/target/release/voiceterm" ]; then
    OVERLAY_BIN="$SCRIPT_DIR/src/target/release/voiceterm"
fi

# Check if Rust overlay exists
if [ -z "$OVERLAY_BIN" ]; then
    echo -e "${YELLOW}Building VoiceTerm (first time setup)...${NC}"
    cd src && cargo build --release --bin voiceterm
    if [ $? -ne 0 ]; then
        echo -e "${RED}Build failed. Please check the error above.${NC}"
        exit 1
    fi
    cd ..
    OVERLAY_BIN="$SCRIPT_DIR/src/target/release/voiceterm"
fi

# Check if whisper model exists
MODEL_PATH=""
DEFAULT_MODELS_DIR="$SCRIPT_DIR/whisper_models"
FALLBACK_MODELS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/voiceterm/models"
MODEL_DIR=""

IS_HOMEBREW=0
case "$SCRIPT_DIR" in
    /opt/homebrew/Cellar/*|/usr/local/Cellar/*) IS_HOMEBREW=1 ;;
esac

if [ -n "${VOICETERM_MODEL_DIR:-}" ]; then
    MODEL_DIR="$VOICETERM_MODEL_DIR"
elif [ "$IS_HOMEBREW" -eq 1 ]; then
    MODEL_DIR="$FALLBACK_MODELS_DIR"
else
    if mkdir -p "$DEFAULT_MODELS_DIR" 2>/dev/null && [ -w "$DEFAULT_MODELS_DIR" ]; then
        MODEL_DIR="$DEFAULT_MODELS_DIR"
    else
        MODEL_DIR="$FALLBACK_MODELS_DIR"
    fi
fi

HAS_WHISPER_ARG=0
for arg in "$@"; do
    case "$arg" in
        --whisper-model-path|--whisper-model-path=*)
            HAS_WHISPER_ARG=1
            ;;
    esac
done

find_model() {
    local search_dir="$1"
    for candidate in \
        "ggml-small.en.bin" \
        "ggml-small.bin" \
        "ggml-base.en.bin" \
        "ggml-base.bin" \
        "ggml-tiny.en.bin" \
        "ggml-tiny.bin"; do
        if [ -f "$search_dir/$candidate" ]; then
            echo "$search_dir/$candidate"
            return 0
        fi
    done
    return 1
}

MODEL_PATH="$(find_model "$DEFAULT_MODELS_DIR" || true)"
if [ -z "$MODEL_PATH" ] && [ "$MODEL_DIR" != "$DEFAULT_MODELS_DIR" ]; then
    MODEL_PATH="$(find_model "$MODEL_DIR" || true)"
fi
if [ -z "$MODEL_PATH" ]; then
    echo -e "${YELLOW}Downloading Whisper model (first time setup)...${NC}"
    VOICETERM_MODEL_DIR="$MODEL_DIR" ./scripts/setup.sh models --base
    if [ $? -ne 0 ]; then
        echo -e "${RED}Model download failed. Please check the error above.${NC}"
        exit 1
    fi
    MODEL_PATH="$(find_model "$MODEL_DIR" || true)"
fi

if [ -z "$MODEL_PATH" ]; then
    echo -e "${RED}Whisper model not found. Run: ./scripts/setup.sh models --base${NC}"
    exit 1
fi
MODEL_PATH_ABS="$MODEL_PATH"

if [ -z "$OVERLAY_BIN" ]; then
    echo -e "${RED}Overlay binary not found. Please run ./scripts/install.sh or build src.${NC}"
    exit 1
fi
EXTRA_ARGS=()
if [ $HAS_WHISPER_ARG -eq 0 ]; then
    EXTRA_ARGS+=(--whisper-model-path "$MODEL_PATH_ABS")
fi
"$OVERLAY_BIN" "${EXTRA_ARGS[@]}" "$@"
