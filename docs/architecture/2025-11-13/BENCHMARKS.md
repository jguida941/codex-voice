# Phase 2A Benchmarks — 2025-11-13

Deterministic measurements collected with `scripts/benchmark_voice.sh`. The script drives the
new `voice_benchmark` binary (synthetic PCM clips + `audio::offline_capture_from_pcm`) so we can
exercise the silence-aware state machine without hardware microphones. Each clip uses a 440 Hz sine
wave for the speech segment plus 700 ms of trailing zeros—this extra 200 ms ensures Earshot has
slack to accumulate the required 500 ms of consecutive silence before issuing `vad_silence`.

## Config Snapshot
- `--voice-vad-engine earshot` (default when the `vad_earshot` feature is enabled)
- `--voice-silence-tail-ms 500`
- `--voice-vad-threshold-db -40`
- `--voice-vad-frame-ms 20`
- `--voice-min-speech-ms-before-stt 300`

## Results
| clip | capture_ms | speech_ms | silence_tail_ms | frames_processed | early_stop |
| --- | --- | --- | --- | --- | --- |
| short | 1560 | 1060 | 500 | 78 | vad_silence |
| medium | 3560 | 3060 | 500 | 178 | vad_silence |
| long | 8560 | 8060 | 500 | 428 | vad_silence |

- **Target clips (<3 s speech)**: short + medium runs produce capture windows within
  `speech_ms + silence_tail_ms`. Based on these measurements we set a conservative SLA of
  `capture_ms ≤ 1.8 s` for short utterances and `capture_ms ≤ 4.2 s` for anything under 3 s of
  speech—roughly 20 % headroom above the observed 1.56 s / 3.56 s values.
- **Long clip**: included for reference only (Phase 2A is not expected to trim 8 s speech).
  The capture loop still stops once the VAD observes the 500 ms trailing silence, which keeps
  `capture_ms` tightly bounded even for long utterances.

Bench logs are stored in `${TMPDIR}/codex_voice_tui.log` and parsed via
`.github/scripts/verify_perf_metrics.py` when perf smoke runs. The same schema appears in
`voice_metrics|capture_ms=…|…` lines, so the perf workflow can adopt the new SLA limits once we
promote them from this manual benchmark.
