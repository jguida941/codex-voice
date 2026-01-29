#!/bin/bash
#
# Codex Voice - Quick Start
# Double-click this file or run: ./start.sh
#

# Save the user's current directory so codex-voice works on their project
export CODEX_VOICE_CWD="$(pwd)"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
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

TERM_COLS="${CODEX_VOICE_FORCE_COLUMNS:-${COLUMNS:-$(tput cols 2>/dev/null || true)}}"
if ! [ "$TERM_COLS" -gt 0 ] 2>/dev/null; then
    TERM_COLS=80
fi
TERM_LINES="${CODEX_VOICE_FORCE_LINES:-${LINES:-$(tput lines 2>/dev/null || true)}}"
if ! [ "$TERM_LINES" -gt 0 ] 2>/dev/null; then
    TERM_LINES=24
fi

print_large_banner() {
    cat <<'BANNER'
   ██████╗ ██████╗ ██████╗ ███████╗██╗  ██╗
  ██╔════╝██╔═══██╗██╔══██╗██╔════╝╚██╗██╔╝
  ██║     ██║   ██║██║  ██║█████╗   ╚███╔╝
  ██║     ██║   ██║██║  ██║██╔══╝   ██╔██╗
  ╚██████╗╚██████╔╝██████╔╝███████╗██╔╝ ██╗
   ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝
          ██╗   ██╗ ██████╗ ██╗ ██████╗███████╗
          ██║   ██║██╔═══██╗██║██╔════╝██╔════╝
          ██║   ██║██║   ██║██║██║     █████╗
          ╚██╗ ██╔╝██║   ██║██║██║     ██╔══╝
           ╚████╔╝ ╚██████╔╝██║╚██████╗███████╗
            ╚═══╝   ╚═════╝ ╚═╝ ╚═════╝╚══════╝
BANNER
}

print_small_banner() {
    cat <<'BANNER'
  ┌───────────────────────────────────────┐
  │  CODEX VOICE                          │
  │  Rust overlay wrapping Codex CLI      │
  │  Speak to Codex with Whisper STT      │
  └───────────────────────────────────────┘
BANNER
}

print_banner() {
    if [ "$TERM_LINES" -ge 33 ] && [ "$TERM_COLS" -ge 80 ]; then
        print_large_banner
    else
        print_small_banner
    fi
}

truncate() {
    local value="$1"
    local width="$2"

    if [ "$width" -le 0 ]; then
        printf ""
        return
    fi
    if [ ${#value} -le "$width" ]; then
        printf "%s" "$value"
        return
    fi
    if [ "$width" -le 3 ]; then
        printf "%.*s" "$width" "$value"
        return
    fi
    printf "%s" "${value:0:$((width - 3))}..."
}

echo ""
echo -e "${CORAL}"
print_banner
echo -e "${NC}"
echo -e "${CORAL_BRIGHT}Starting Codex Voice...${NC}"
echo ""
EXAMPLE_CMD="codex-voice"
if ! command -v codex-voice &> /dev/null; then
    EXAMPLE_CMD="./start.sh"
fi

print_controls_table_wide() {
    local col_key=9
    local col_action=$(( (TERM_COLS - 13 - (col_key * 2)) / 2 ))
    local border_key
    local border_action
    local row

    if [ "$col_action" -lt 14 ]; then
        col_action=14
    fi

    border_key=$(printf '%*s' "$col_key" '' | tr ' ' '─')
    border_action=$(printf '%*s' "$col_action" '' | tr ' ' '─')

    printf "${CORAL}╭─%s─┬─%s─┬─%s─┬─%s─╮${NC}\n" "$border_key" "$border_action" "$border_key" "$border_action"
    printf "${CORAL}│${CORAL_BRIGHT}${BOLD} %-*s ${CORAL}│${CORAL_BRIGHT}${BOLD} %-*s ${CORAL}│${GOLD}${BOLD} %-*s ${CORAL}│${GOLD}${BOLD} %-*s ${CORAL}│${NC}\n" \
        "$col_key" "Control" "$col_action" "Action" "$col_key" "Control" "$col_action" "Action"
    printf "${CORAL}├─%s─┼─%s─┼─%s─┼─%s─┤${NC}\n" "$border_key" "$border_action" "$border_key" "$border_action"

    for row in \
        "Ctrl+R|Record (push-to-talk)|Ctrl+V|Toggle auto-voice" \
        "Ctrl+T|Toggle send mode|Ctrl+]|Mic sensitivity +5 dB" \
        "Ctrl+\\|Mic sensitivity -5 dB|Ctrl+Q|Quit overlay"; do
        IFS='|' read -r key_left action_left key_right action_right <<< "$row"
        key_left="$(truncate "$key_left" "$col_key")"
        action_left="$(truncate "$action_left" "$col_action")"
        key_right="$(truncate "$key_right" "$col_key")"
        action_right="$(truncate "$action_right" "$col_action")"
        printf "${CORAL}│${NC} %-*s ${CORAL}│${NC} %-*s ${CORAL}│${GOLD} %-*s ${CORAL}│${GOLD} %-*s ${CORAL}│${NC}\n" \
            "$col_key" "$key_left" "$col_action" "$action_left" \
            "$col_key" "$key_right" "$col_action" "$action_right"
    done

    printf "${CORAL}╰─%s─┴─%s─┴─%s─┴─%s─╯${NC}\n" "$border_key" "$border_action" "$border_key" "$border_action"
}

print_controls_table_narrow() {
    local col1=12
    local col2=$((TERM_COLS - col1 - 7))
    local border1
    local border2
    local row

    if [ "$col2" -lt 10 ]; then
        col2=10
    fi

    border1=$(printf '%*s' "$col1" '' | tr ' ' '─')
    border2=$(printf '%*s' "$col2" '' | tr ' ' '─')

    printf "${CORAL}╭─%s─┬─%s─╮${NC}\n" "$border1" "$border2"
    printf "${CORAL}│${CORAL_BRIGHT}${BOLD} %-*s ${CORAL}│${GOLD}${BOLD} %-*s ${CORAL}│${NC}\n" "$col1" "Control" "$col2" "Action"
    printf "${CORAL}├─%s─┼─%s─┤${NC}\n" "$border1" "$border2"

    for row in \
        "Ctrl+R|Record (push-to-talk)" \
        "Ctrl+V|Toggle auto-voice" \
        "Ctrl+T|Toggle send mode" \
        "Ctrl+]|Mic sensitivity +5 dB" \
        "Ctrl+\\|Mic sensitivity -5 dB" \
        "Ctrl+Q|Quit overlay"; do
        IFS='|' read -r key action <<< "$row"
        key="$(truncate "$key" "$col1")"
        action="$(truncate "$action" "$col2")"
        printf "${CORAL}│${NC} %-*s ${CORAL}│${GOLD} %-*s ${CORAL}│${NC}\n" "$col1" "$key" "$col2" "$action"
    done

    printf "${CORAL}╰─%s─┴─%s─╯${NC}\n" "$border1" "$border2"
}

print_controls_table() {
    if [ "$TERM_COLS" -ge 90 ]; then
        print_controls_table_wide
    else
        print_controls_table_narrow
    fi
}

print_commands_table() {
    local col1=46
    local col2=24
    local border1
    local border2
    local row

    if [ "$TERM_COLS" -lt $((col1 + col2 + 7)) ]; then
        col2=20
        col1=$((TERM_COLS - col2 - 7))
    fi
    if [ "$col1" -lt 24 ]; then
        col1=24
        col2=$((TERM_COLS - col1 - 7))
    fi
    if [ "$col2" -lt 14 ]; then
        col2=14
        col1=$((TERM_COLS - col2 - 7))
    fi

    border1=$(printf '%*s' "$col1" '' | tr ' ' '─')
    border2=$(printf '%*s' "$col2" '' | tr ' ' '─')

    printf "${CORAL}╭─%s─┬─%s─╮${NC}\n" "$border1" "$border2"
    printf "${CORAL}│${CORAL_BRIGHT}${BOLD} %-*s ${CORAL}│${GOLD}${BOLD} %-*s ${CORAL}│${NC}\n" "$col1" "Command" "$col2" "Purpose"
    printf "${CORAL}├─%s─┼─%s─┤${NC}\n" "$border1" "$border2"

    for row in \
        "$EXAMPLE_CMD --auto-voice|Start in auto-voice" \
        "$EXAMPLE_CMD --voice-send-mode insert|Start in insert mode" \
        "$EXAMPLE_CMD --mic-meter|Measure ambient/speech levels" \
        "$EXAMPLE_CMD --voice-vad-threshold-db -50|Set mic threshold" \
        "$EXAMPLE_CMD --auto-voice-idle-ms 700|Auto-voice idle" \
        "$EXAMPLE_CMD --transcript-idle-ms 250|Transcript idle"; do
        IFS='|' read -r command purpose <<< "$row"
        command="$(truncate "$command" "$col1")"
        purpose="$(truncate "$purpose" "$col2")"
        printf "${CORAL}│${NC} %-*s ${CORAL}│${GOLD} %-*s ${CORAL}│${NC}\n" "$col1" "$command" "$col2" "$purpose"
    done

    printf "${CORAL}╰─%s─┴─%s─╯${NC}\n" "$border1" "$border2"
}

echo -e "${CORAL_BRIGHT}${BOLD}Quick Controls${NC}"
print_controls_table
echo -e "${DIM}Sensitivity: Ctrl+] (less sensitive) • Ctrl+\\ or Ctrl+/ (more sensitive)${NC}"
echo ""
echo -e "${CORAL_BRIGHT}${BOLD}Common Commands${NC}"
print_commands_table
echo -e "${DIM}Auto-voice idle default: 1200ms • Transcript idle default: 250ms${NC}"
echo ""

# Startup output-only mode for tests
if [ "${CODEX_VOICE_STARTUP_ONLY:-0}" = "1" ]; then
    exit 0
fi

# Resolve binary (prefer local build; avoid wrapper recursion)
OVERLAY_BIN=""
if [ -x "$SCRIPT_DIR/rust_tui/target/release/codex-voice" ]; then
    OVERLAY_BIN="$SCRIPT_DIR/rust_tui/target/release/codex-voice"
fi

# Check if Rust overlay exists
if [ -z "$OVERLAY_BIN" ]; then
    echo -e "${YELLOW}Building Codex Voice (first time setup)...${NC}"
    cd rust_tui && cargo build --release --bin codex-voice
    if [ $? -ne 0 ]; then
        echo -e "${RED}Build failed. Please check the error above.${NC}"
        exit 1
    fi
    cd ..
    OVERLAY_BIN="$SCRIPT_DIR/rust_tui/target/release/codex-voice"
fi

# Check if whisper model exists
MODEL_PATH=""
DEFAULT_MODELS_DIR="$SCRIPT_DIR/models"
FALLBACK_MODELS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/codex-voice/models"
MODEL_DIR=""

IS_HOMEBREW=0
case "$SCRIPT_DIR" in
    /opt/homebrew/Cellar/*|/usr/local/Cellar/*) IS_HOMEBREW=1 ;;
esac

if [ -n "${CODEX_VOICE_MODEL_DIR:-}" ]; then
    MODEL_DIR="$CODEX_VOICE_MODEL_DIR"
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
    CODEX_VOICE_MODEL_DIR="$MODEL_DIR" ./scripts/setup.sh models --base
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

echo -e "${CORAL_BRIGHT}Launching overlay mode...${NC}"
if [ -z "$OVERLAY_BIN" ]; then
    echo -e "${RED}Overlay binary not found. Please run ./install.sh or build rust_tui.${NC}"
    exit 1
fi
EXTRA_ARGS=()
if [ $HAS_WHISPER_ARG -eq 0 ]; then
    EXTRA_ARGS+=(--whisper-model-path "$MODEL_PATH_ABS")
fi
"$OVERLAY_BIN" "${EXTRA_ARGS[@]}" "$@"
