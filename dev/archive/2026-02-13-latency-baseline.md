# Latency Baseline (Synthetic, CI-Friendly)

Date: 2026-02-13  
Plan items: MP-083 (baseline measurement), MP-084 (synthetic guardrails input)

## Goal
Establish a repeatable baseline for voice-pipeline latency behavior using the
`latency_measurement` harness through `dev/scripts/tests/measure_latency.sh`.

This baseline intentionally uses synthetic clips with `--skip-stt` so it is:
- deterministic across runs,
- runnable without microphone access,
- runnable without a local Whisper model.

## Command

```bash
./dev/scripts/tests/measure_latency.sh --synthetic --voice-only --skip-stt --count 5
```

## Results

Two synthetic utterance profiles were collected:

| Profile | Speech | Silence | Samples | voice_capture_ms | voice_total_ms |
|---|---:|---:|---:|---:|---:|
| short | 1000ms | 700ms | 5 | 1700 | 1700 |
| medium | 3000ms | 700ms | 5 | 3700 | 3700 |

Observed variance for both profiles in this run: `0ms` across all samples.

## Guardrail thresholds derived from baseline

These ranges are now enforced in CI (`latency_guard.yml`) via
`./dev/scripts/tests/measure_latency.sh --ci-guard`:

- short profile: `voice_capture_ms` and `voice_total_ms` in `[1200, 2800]`
- medium profile: `voice_capture_ms` and `voice_total_ms` in `[3200, 5200]`

These bounds are intentionally wider than the exact baseline values to absorb
minor environment/config variation while still catching functional regressions.
