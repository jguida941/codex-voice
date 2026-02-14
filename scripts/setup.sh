#!/bin/bash
#
# VoiceTerm Setup Script
# Downloads Whisper models and verifies dependencies
#
# Supported platforms: macOS (Intel/Apple Silicon), Linux (x86_64/arm64)
#

set -e

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Darwin) PLATFORM="macos" ;;
    Linux)  PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "Windows is not yet supported. Try WSL2 with Linux instructions."
        exit 1
        ;;
    *)
        echo "Unsupported operating system: $OS"
        exit 1
        ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DEFAULT_MODELS_DIR="$PROJECT_ROOT/whisper_models"
FALLBACK_MODELS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/voiceterm/models"

IS_HOMEBREW=0
case "$PROJECT_ROOT" in
    /opt/homebrew/Cellar/*|/usr/local/Cellar/*) IS_HOMEBREW=1 ;;
esac

if [ -n "${VOICETERM_MODEL_DIR:-}" ]; then
    MODELS_DIR="$VOICETERM_MODEL_DIR"
elif [ "$IS_HOMEBREW" -eq 1 ]; then
    MODELS_DIR="$FALLBACK_MODELS_DIR"
else
    if mkdir -p "$DEFAULT_MODELS_DIR" 2>/dev/null && [ -w "$DEFAULT_MODELS_DIR" ]; then
        MODELS_DIR="$DEFAULT_MODELS_DIR"
    else
        MODELS_DIR="$FALLBACK_MODELS_DIR"
    fi
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_banner() {
    local text="$1"
    local width=50
    local text_len=${#text}
    local padding=$(( (width - text_len) / 2 ))
    local line=$(printf '━%.0s' $(seq 1 $width))
    echo ""
    echo -e "${GREEN}${line}${NC}"
    printf "${GREEN}%*s%s%*s${NC}\n" $padding "" "$text" $((width - padding - text_len)) ""
    echo -e "${GREEN}${line}${NC}"
    echo ""
}

print_header() {
    print_banner "VoiceTerm Setup"
}

print_step() {
    echo -e "${BLUE}▶${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Whisper model URLs from HuggingFace
WHISPER_BASE_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main"

# Get model size (compatible with bash 3)
get_model_size() {
    case "$1" in
        tiny.en|tiny) echo "75M" ;;
        base.en|base) echo "142M" ;;
        small.en|small) echo "466M" ;;
        medium.en|medium) echo "1.5G" ;;
        large) echo "3.1G" ;;
        *) echo "unknown" ;;
    esac
}

resolve_single_model() {
    case "$1" in
        ""|--base|base|base.en) echo "base.en" ;;
        --tiny|tiny|tiny.en) echo "tiny.en" ;;
        --small|small|small.en) echo "small.en" ;;
        --medium|medium|medium.en) echo "medium.en" ;;
        *)
            print_error "Unknown model option: $1"
            show_usage
            exit 1
            ;;
    esac
}

download_whisper_model() {
    local model_name="$1"
    local model_file="ggml-${model_name}.bin"
    local model_path="$MODELS_DIR/$model_file"
    local model_url="$WHISPER_BASE_URL/$model_file"
    local model_size=$(get_model_size "$model_name")

    if [ -f "$model_path" ]; then
        print_success "Model '$model_name' already exists at $model_path"
        return 0
    fi

    print_step "Downloading Whisper model: $model_name ($model_size)"

    mkdir -p "$MODELS_DIR"

    if command -v curl &> /dev/null; then
        curl -L --progress-bar "$model_url" -o "$model_path" || {
            print_error "Failed to download $model_name"
            rm -f "$model_path"
            return 1
        }
    elif command -v wget &> /dev/null; then
        wget -q --show-progress "$model_url" -O "$model_path" || {
            print_error "Failed to download $model_name"
            rm -f "$model_path"
            return 1
        }
    else
        print_error "Neither curl nor wget found. Please install one of them."
        return 1
    fi

    print_success "Downloaded $model_file"
}

check_rust() {
    print_step "Checking Rust toolchain..."

    if command -v cargo &> /dev/null; then
        local rust_version=$(rustc --version 2>/dev/null || echo "unknown")
        print_success "Rust found: $rust_version"
        return 0
    else
        print_error "Rust not found. Please install it from https://rustup.rs"
        return 1
    fi
}

check_codex() {
    print_step "Checking Codex CLI..."

    if command -v codex &> /dev/null; then
        print_success "Codex CLI found in PATH"
        return 0
    else
        print_warning "Codex CLI not found in PATH"
        echo "         Install from: npm install -g @openai/codex"
        return 0
    fi
}

build_rust_overlay() {
    print_step "Building Rust overlay..."

    cd "$PROJECT_ROOT/src"

    if cargo build --release --bin voiceterm; then
        print_success "Rust overlay built successfully"
        return 0
    else
        print_error "Failed to build Rust overlay"
        return 1
    fi
}

install_wrapper() {
    local install_dir=""
    local wrapper_path=""

    if [ -n "${VOICETERM_INSTALL_DIR:-}" ]; then
        install_dir="$VOICETERM_INSTALL_DIR"
        wrapper_path="$install_dir/voiceterm"
    else
        local candidates=(
            "/opt/homebrew/bin"
            "/usr/local/bin"
            "$HOME/.local/bin"
            "$PROJECT_ROOT/bin"
        )

        for candidate in "${candidates[@]}"; do
            if mkdir -p "$candidate" 2>/dev/null && [ -w "$candidate" ]; then
                local candidate_path="$candidate/voiceterm"
                if [ -e "$candidate_path" ]; then
                    case "$candidate" in
                        "$HOME/.local/bin"|"$PROJECT_ROOT/bin")
                            install_dir="$candidate"
                            wrapper_path="$candidate_path"
                            break
                            ;;
                        *)
                            print_warning "Found existing $candidate_path; skipping."
                            continue
                            ;;
                    esac
                fi
                install_dir="$candidate"
                wrapper_path="$candidate_path"
                break
            fi
        done
    fi

    if [ -z "$install_dir" ]; then
        print_error "No writable install directory found."
        print_warning "Set VOICETERM_INSTALL_DIR to a writable path and rerun."
        return 1
    fi

    wrapper_path="${wrapper_path:-$install_dir/voiceterm}"

    print_step "Installing voiceterm wrapper into $install_dir"

    mkdir -p "$install_dir"
cat > "$wrapper_path" <<EOF
#!/bin/bash
export VOICETERM_CWD="\$(pwd)"
exec "$PROJECT_ROOT/scripts/start.sh" "\$@"
EOF
    chmod 0755 "$wrapper_path"

    print_success "Installed $wrapper_path"

    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *)
            print_warning "Add $install_dir to PATH to run 'voiceterm' from anywhere."
            echo "         Example: echo 'export PATH=\"$install_dir:\$PATH\"' >> ~/.zshrc"
            ;;
    esac
}

show_usage() {
    echo "Usage: $0 [command] [options]"
    echo ""
    echo "Commands:"
    echo "  install          Full overlay install (model + build + wrapper) (recommended, default)"
    echo "  overlay          Setup overlay only (model + build)"
    echo "  models           Download Whisper models only"
    echo "  check            Check dependencies only"
    echo "  build            Build Rust overlay only"
    echo ""
    echo "Model options (for 'models' command):"
    echo "  --tiny           Download tiny.en model (75M, fastest)"
    echo "  --base           Download base.en model (142M, recommended)"
    echo "  --small          Download small.en model (466M)"
    echo "  --medium         Download medium.en model (1.5G)"
    echo "  --all-models     Download all English models"
    echo ""
    echo "Examples:"
    echo "  $0                    # Full setup with base.en model"
    echo "  $0 models --tiny      # Download only tiny.en model"
    echo "  $0 check              # Check dependencies only"
    echo "  $0 build              # Build only (skip model download)"
}

main() {
    local command="${1:-install}"
    shift || true

    print_header

    case "$command" in
        install)
            local model
            model="$(resolve_single_model "${1:-}")"

            check_rust || exit 1
            check_codex

            echo ""
            download_whisper_model "$model"

            echo ""
            build_rust_overlay || exit 1

            echo ""
            install_wrapper

            print_banner "✓ Install Complete!"
            echo ""
            echo "Run from any project:"
            echo "  voiceterm"
            echo ""
            ;;

        overlay)
            local model
            model="$(resolve_single_model "${1:-}")"

            check_rust || exit 1
            check_codex

            echo ""
            download_whisper_model "$model"

            echo ""
            build_rust_overlay || exit 1

            print_banner "✓ Overlay Ready!"
            echo ""
            echo "To start VoiceTerm:"
            echo "  ./scripts/start.sh"
            echo ""
            ;;

        models)
            local models_to_download=()

            while [[ $# -gt 0 ]]; do
                case "$1" in
                    --tiny)
                        models_to_download+=("tiny.en")
                        ;;
                    --base)
                        models_to_download+=("base.en")
                        ;;
                    --small)
                        models_to_download+=("small.en")
                        ;;
                    --medium)
                        models_to_download+=("medium.en")
                        ;;
                    --all-models)
                        models_to_download+=("tiny.en" "base.en" "small.en" "medium.en")
                        ;;
                    *)
                        print_error "Unknown option: $1"
                        show_usage
                        exit 1
                        ;;
                esac
                shift
            done

            # Default to base.en if no model specified
            if [ ${#models_to_download[@]} -eq 0 ]; then
                models_to_download=("base.en")
            fi

            for model in "${models_to_download[@]}"; do
                download_whisper_model "$model"
            done

            print_success "Model download complete!"
            ;;

        check)
            check_rust
            check_codex

            echo ""
            print_step "Checking Whisper models..."
            if ls "$MODELS_DIR"/ggml-*.bin 1> /dev/null 2>&1; then
                for model in "$MODELS_DIR"/ggml-*.bin; do
                    local size=$(du -h "$model" | cut -f1)
                    print_success "Found: $(basename "$model") ($size)"
                done
            else
                print_warning "No Whisper models found in $MODELS_DIR"
                echo "         Run: $0 models --base"
            fi
            ;;

        build)
            build_rust_overlay || exit 1

            print_success "Build complete!"
            ;;

        help|--help|-h)
            show_usage
            ;;

        *)
            print_error "Unknown command: $command"
            show_usage
            exit 1
            ;;
    esac
}

main "$@"
