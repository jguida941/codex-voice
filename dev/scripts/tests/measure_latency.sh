#!/usr/bin/env bash
# Full pipeline latency measurement script for Phase 2B measurement gate.
#
# This script collects latency data for the voice→Codex round-trip to identify
# the primary bottleneck. Run this BEFORE implementing Phase 2B to ensure the
# optimization targets the right component.
#
# Usage:
#   ./dev/scripts/tests/measure_latency.sh               # Interactive mode, 10 samples
#   ./dev/scripts/tests/measure_latency.sh --synthetic   # Synthetic audio mode
#   ./dev/scripts/tests/measure_latency.sh --voice-only  # Measure voice pipeline only
#   ./dev/scripts/tests/measure_latency.sh --ci-guard    # CI-friendly synthetic latency guardrails

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT/src"

# Parse arguments
MODE="interactive"
COUNT=10
VOICE_ONLY_ARGS=()
SKIP_STT_ARGS=()
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case $1 in
    --ci-guard)
      MODE="ci-guard"
      shift
      ;;
    --synthetic)
      MODE="synthetic"
      shift
      ;;
    --voice-only)
      VOICE_ONLY_ARGS+=(--voice-only)
      shift
      ;;
    --skip-stt)
      SKIP_STT_ARGS+=(--skip-stt)
      shift
      ;;
    --count)
      COUNT="$2"
      shift 2
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

if [[ "$MODE" != "synthetic" && "$MODE" != "ci-guard" && ${#SKIP_STT_ARGS[@]} -gt 0 ]]; then
  echo "error: --skip-stt requires --synthetic or --ci-guard" >&2
  exit 2
fi

echo "=================================="
echo "Phase 2B Latency Measurement Gate"
echo "=================================="
echo ""
echo "This measurement identifies whether voice or Codex is the bottleneck."
echo "Collecting $COUNT samples in $MODE mode..."
echo ""

if [[ "$MODE" == "ci-guard" ]]; then
  echo "Running CI-friendly synthetic latency guardrails (no mic, no Whisper model)..."
  echo ""

  cargo run --quiet --release --bin latency_measurement -- \
    --label "ci-short" \
    --count "$COUNT" \
    --synthetic \
    --speech-ms 1000 \
    --silence-ms 700 \
    --voice-only \
    --skip-stt \
    --min-voice-capture-ms 1200 \
    --max-voice-capture-ms 2600 \
    --min-voice-total-ms 1200 \
    --max-voice-total-ms 2800 \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}

  echo ""
  echo "---"
  echo ""

  cargo run --quiet --release --bin latency_measurement -- \
    --label "ci-medium" \
    --count "$COUNT" \
    --synthetic \
    --speech-ms 3000 \
    --silence-ms 700 \
    --voice-only \
    --skip-stt \
    --min-voice-capture-ms 3200 \
    --max-voice-capture-ms 5000 \
    --min-voice-total-ms 3200 \
    --max-voice-total-ms 5200 \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}

elif [[ "$MODE" == "synthetic" ]]; then
  echo "Running synthetic measurements (short, medium utterances)..."
  echo ""

  # Short utterance (1s speech)
  cargo run --quiet --release --bin latency_measurement -- \
    --label "short" \
    --count "$COUNT" \
    --synthetic \
    --speech-ms 1000 \
    --silence-ms 700 \
    "${VOICE_ONLY_ARGS[@]}" \
    "${SKIP_STT_ARGS[@]}" \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}

  echo ""
  echo "---"
  echo ""

  # Medium utterance (3s speech)
  cargo run --quiet --release --bin latency_measurement -- \
    --label "medium" \
    --count "$COUNT" \
    --synthetic \
    --speech-ms 3000 \
    --silence-ms 700 \
    "${VOICE_ONLY_ARGS[@]}" \
    "${SKIP_STT_ARGS[@]}" \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}

else
  echo "Interactive mode: You will be prompted to speak $COUNT times."
  echo "Speak naturally for 1-3 seconds each time."
  echo ""
  read -p "Press Enter when ready to start measurements..."
  echo ""

  cargo run --release --bin latency_measurement -- \
    --label "interactive" \
    --count "$COUNT" \
    "${VOICE_ONLY_ARGS[@]}" \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}
fi

echo ""
echo "=================================="
echo "Measurement Complete"
echo "=================================="
echo ""
echo "Next steps:"
echo "1. Review the analysis above"
echo "2. Document results in dev/archive/ or dev/active/"
echo "3. Decide on Phase 2B approach based on bottleneck:"
echo "   - If Codex >70%: Consider deferring Phase 2B"
echo "   - If Voice >50%: Proceed with streaming architecture (Option B)"
echo "   - If balanced: Phase 2B may provide noticeable improvement"
echo ""
