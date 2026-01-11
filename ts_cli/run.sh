#!/bin/bash
# Convenience script to run Codex Voice CLI

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
RUST_DIR="$ROOT_DIR/rust_tui"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Codex Voice CLI${NC}"
echo "─────────────────────────────────────"

# Check if Rust backend is built
RUST_BINARY="$RUST_DIR/target/release/rust_tui"
if [ ! -f "$RUST_BINARY" ]; then
    echo -e "${YELLOW}Building Rust backend...${NC}"
    cd "$RUST_DIR"
    cargo build --release
    echo -e "${GREEN}Rust backend built successfully${NC}"
fi

# Check if TypeScript CLI is built
TS_ENTRY="$SCRIPT_DIR/dist/index.js"
if [ ! -f "$TS_ENTRY" ]; then
    echo -e "${YELLOW}Building TypeScript CLI...${NC}"
    cd "$SCRIPT_DIR"
    npm install
    npm run build
    echo -e "${GREEN}TypeScript CLI built successfully${NC}"
fi

# Run the CLI
cd "$SCRIPT_DIR"
node dist/index.js "$@"
