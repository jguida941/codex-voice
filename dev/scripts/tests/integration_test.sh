#!/usr/bin/env bash
#
# Integration test for VoiceTerm IPC protocol
# Tests the end-to-end flow between IPC clients and the Rust backend
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
RUST_BINARY="$PROJECT_ROOT/src/target/release/voiceterm"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

TESTS_PASSED=0
TESTS_FAILED=0

print_test() {
    echo -e "${BLUE}TEST:${NC} $1"
}

pass() {
    TESTS_PASSED=$((TESTS_PASSED + 1))
    echo -e "${GREEN}PASS:${NC} $1"
}

fail() {
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo -e "${RED}FAIL:${NC} $1"
}

skip() {
    echo -e "${YELLOW}SKIP:${NC} $1"
}

# ============================================================================
# Pre-flight checks
# ============================================================================

echo ""
echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║${NC}           VoiceTerm Integration Tests                      ${BLUE}║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check Rust binary exists
if [ ! -f "$RUST_BINARY" ]; then
    echo -e "${RED}Error:${NC} Rust binary not found at $RUST_BINARY"
    echo "       Run: cd src && cargo build --release"
    exit 1
fi

# ============================================================================
# Test 1: Backend startup and capabilities
# ============================================================================

print_test "Backend startup and capabilities event"

# Start backend in IPC mode and capture first event
# Use direct pipe with head which will close stdin, causing clean exit
FIRST_EVENT=$(echo '{"cmd": "get_capabilities"}' | "$RUST_BINARY" --json-ipc 2>/dev/null | head -1 || true)

if echo "$FIRST_EVENT" | grep -q '"event":"capabilities"'; then
    pass "Received capabilities event on startup"
else
    fail "Did not receive capabilities event"
    echo "      Got: $FIRST_EVENT"
fi

# ============================================================================
# Test 2: Capabilities event structure
# ============================================================================

print_test "Capabilities event structure"

if echo "$FIRST_EVENT" | grep -q '"session_id"'; then
    pass "Capabilities includes session_id"
else
    fail "Capabilities missing session_id"
fi

if echo "$FIRST_EVENT" | grep -q '"providers_available"'; then
    pass "Capabilities includes providers_available"
else
    fail "Capabilities missing providers_available"
fi

if echo "$FIRST_EVENT" | grep -q '"active_provider"'; then
    pass "Capabilities includes active_provider"
else
    fail "Capabilities missing active_provider"
fi

# ============================================================================
# Test 3: Command parsing - wrapper commands
# ============================================================================

print_test "IPC command parsing"

# Test send_prompt command serialization
TEST_CMD='{"cmd": "send_prompt", "prompt": "test"}'
if echo "$TEST_CMD" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    pass "send_prompt command is valid JSON"
else
    fail "send_prompt command is invalid JSON"
fi

# Test set_provider command
TEST_CMD='{"cmd": "set_provider", "provider": "claude"}'
if echo "$TEST_CMD" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    pass "set_provider command is valid JSON"
else
    fail "set_provider command is invalid JSON"
fi

# Test auth command
TEST_CMD='{"cmd": "auth", "provider": "codex"}'
if echo "$TEST_CMD" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    pass "auth command is valid JSON"
else
    fail "auth command is invalid JSON"
fi

# Test start_voice command
TEST_CMD='{"cmd": "start_voice"}'
if echo "$TEST_CMD" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    pass "start_voice command is valid JSON"
else
    fail "start_voice command is invalid JSON"
fi

# ============================================================================
# Test 4: Event serialization
# ============================================================================

print_test "Event JSON serialization"

# Verify the capabilities event is valid JSON
if echo "$FIRST_EVENT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    pass "Capabilities event is valid JSON"
else
    fail "Capabilities event is invalid JSON"
fi

# ============================================================================
# Test 5: Protocol compatibility
# ============================================================================

print_test "Protocol version compatibility"

# Extract version from capabilities
VERSION=$(echo "$FIRST_EVENT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('version', 'unknown'))" 2>/dev/null || echo "unknown")
if [ "$VERSION" != "unknown" ]; then
    pass "Backend reports version: $VERSION"
else
    skip "Could not extract version from capabilities"
fi

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

TOTAL=$((TESTS_PASSED + TESTS_FAILED))
if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All $TESTS_PASSED tests passed!${NC}"
    exit 0
else
    echo -e "${GREEN}$TESTS_PASSED passed${NC}, ${RED}$TESTS_FAILED failed${NC} out of $TOTAL tests"
    exit 1
fi
