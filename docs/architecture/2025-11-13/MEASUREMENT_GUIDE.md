# Latency Measurement Guide

## Quick Start

The easiest way to collect measurements is using the wrapper script:

```bash
# Interactive mode: real microphone, 10 samples
./scripts/measure_latency.sh

# Synthetic mode: deterministic test clips
./scripts/measure_latency.sh --synthetic

# Voice pipeline only (skip Codex calls)
./scripts/measure_latency.sh --voice-only

# Custom sample count
./scripts/measure_latency.sh --count 5
```

## Running Measurements Directly

For more control, use the binary directly:

```bash
cd rust_tui

# Interactive mode - you'll be prompted to speak
cargo run --release --bin latency_measurement -- \
  --label "my_test" \
  --count 10

# Synthetic short utterance (1s speech + 700ms silence)
cargo run --release --bin latency_measurement -- \
  --label "short" \
  --synthetic \
  --speech-ms 1000 \
  --silence-ms 700 \
  --count 10

# Voice-only measurement (no Codex calls)
cargo run --release --bin latency_measurement -- \
  --voice-only \
  --synthetic \
  --speech-ms 3000 \
  --silence-ms 700
```

## Output Format

The tool outputs:

1. **Per-measurement table:**
   ```
   | label | voice_capture_ms | voice_stt_ms | voice_total_ms | codex_ms | total_ms | ...
   ```

2. **Aggregate analysis:**
   - Average latencies
   - Bottleneck percentages
   - Recommendations based on thresholds

## Collecting Data for LATENCY_MEASUREMENTS.md

**Recommended procedure:**

1. **Run synthetic baseline:**
   ```bash
   ./scripts/measure_latency.sh --synthetic --count 10 > /tmp/synthetic_results.txt
   ```

2. **Run real microphone tests:**
   ```bash
   ./scripts/measure_latency.sh --count 10 > /tmp/real_results.txt
   ```

3. **Copy results to documentation:**
   - Open `/tmp/synthetic_results.txt` and `/tmp/real_results.txt`
   - Paste output into `LATENCY_MEASUREMENTS.md` under "Raw Measurements"

4. **Document configuration:**
   - Record VAD engine (earshot/simple)
   - Record Whisper model path
   - Record Codex backend (pty/cli)
   - Record hardware details (CPU, memory)

5. **Analyze and make decision:**
   - If Codex >70%: defer Phase 2B
   - If Voice >50%: proceed with streaming (Option B)
   - If balanced: consider hybrid/cloud approach

## Troubleshooting

**No Whisper model configured:**
- Set `--whisper-model-path` or measurement falls back to Python
- Python fallback is slower and less accurate for measurement

**Codex calls failing:**
- Use `--voice-only` to measure just the voice pipeline
- Verify `codex` binary is in PATH

**Synthetic mode errors:**
- Requires both `--speech-ms` and `--silence-ms`
- Requires native Whisper model (no Python fallback)

**Real microphone issues:**
- Check audio device permissions
- Try `--input-device <name>` to select specific mic
- Review `$TMPDIR/codex_voice_tui.log` for capture errors

## Measurement Validity

**Good measurements have:**
- ✓ Consistent voice_total_ms (±20% variance acceptable)
- ✓ Codex_ms >1000ms (realistic API latency)
- ✓ At least 10 samples for statistical significance

**Bad measurements indicate:**
- ✗ voice_total_ms < 500ms → likely empty transcript
- ✗ codex_ms < 100ms → test hook active or cached response
- ✗ High variance (>50%) → hardware contention or throttling

## Example Session

```bash
$ ./scripts/measure_latency.sh --synthetic

==================================
Phase 2B Latency Measurement Gate
==================================

Running synthetic measurements (short, medium utterances)...

=== Measurement 1/10 ===
Running synthetic clip: 1000ms speech + 700ms silence
Voice capture: 1560 ms
STT: 820 ms
Transcript: [synthetic audio transcription]

Starting Codex call...
Codex complete: 5420 ms

[... 9 more measurements ...]

=== LATENCY MEASUREMENTS ===

| label | voice_capture_ms | voice_stt_ms | voice_total_ms | codex_ms | total_ms | ...
| short | 1560 | 820 | 2380 | 5420 | 7800 | ...
| short | 1550 | 830 | 2380 | 5210 | 7590 | ...
...

=== ANALYSIS ===

Voice Pipeline:
  Average total: 2380.0 ms

Codex API:
  Average: 5315.0 ms

Total Round-Trip:
  Average: 7695.0 ms

Bottleneck Analysis:
  Voice:  30.9% of total time
  Codex:  69.1% of total time

Recommendations:
  ⚠️  Codex API is the primary bottleneck (69.1%)
  → Voice optimization (Phase 2B) would save <31% of total latency
  → Consider deferring Phase 2B until Codex latency is improved

==================================
Measurement Complete
==================================
```

## Integration with CI

Future enhancement: Add to `.github/workflows/perf_smoke.yml` to track latency regressions.

```yaml
- name: Latency regression check
  run: |
    ./scripts/measure_latency.sh --synthetic --count 5 --voice-only
    # Parse output and fail if voice_total_ms > threshold
```
