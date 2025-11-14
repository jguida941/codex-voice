# Latency Measurements — 2025-11-13

All samples captured using the Rust TUI with the async Codex worker. Each run reflects a single Ctrl+R capture followed by sending the transcript to Codex so the Codex request latency is recorded.

> Columns: `voice_capture_ms` (VAD-controlled recording), `voice_stt_ms` (Whisper batch transcription), `voice_total_ms` (capture + STT), `codex_ms` (Codex worker time), `total_ms` (voice_total_ms + codex_ms), transcript + Codex character counts for basic sanity.

| label       | voice_capture_ms | voice_stt_ms | voice_total_ms | codex_ms | total_ms | transcript_chars | codex_output_chars |
|-------------|------------------|--------------|----------------|----------|----------|------------------|--------------------|
| interactive | 2718 | 222 | 2946 | 12473 | 16635 | 29 | 336 |
| interactive | 605  | 211 | 819  | 5682  | 6502  | 13 | 325 |
| interactive | 1210 | 225 | 1445 | 2013  | 3459  | 11 | 72  |
| interactive | 1303 | 208 | 1518 | 15200 | 16719 | 11 | 345 |
| interactive | 601  | 210 | 815  | 11201 | 12017 | 13 | 291 |
| interactive | 665  | 213 | 885  | 6472  | 7358  | 13 | 156 |
| interactive | 603  | 206 | 812  | 17500 | 18313 | 13 | 391 |
| interactive | 602  | 212 | 819  | 5376  | 6196  | 13 | 218 |
| interactive | 682  | 206 | 889  | 5238  | 6128  | 13 | 271 |
| interactive | 684  | 220 | 911  | 10909 | 11820 | 13 | 211 |

## Summary

| Metric | Average (ms) | Min (ms) | Max (ms) |
|--------|--------------|----------|----------|
| Voice capture | 796.3 | 601 | 2718 |
| Voice STT | 213.3 | 206 | 225 |
| Voice total (capture + STT) | **1185.9** | 812 | 2946 |
| Codex | **9206.4** | 2013 | 17500 |
| End-to-end total | 10514.7 | 3459 | 18313 |

## Bottleneck Analysis

- Voice pipeline accounts for **≈11.3 %** of total latency (1185.9 ms).
- Codex accounts for **≈87.6 %** of total latency (9206.4 ms).
- Even if Phase 2B reduced voice latency to the 750 ms target, total improvement would be <5 % while Codex continues to add 5–17 s per request.

## Implications

1. **Phase 2B (streaming Whisper) should not start yet.** Codex remains the dominant bottleneck and violates the “few hundred milliseconds” round-trip SLA regardless of voice optimizations.
2. **Voice still exceeds the 750 ms target** (current average 1185.9 ms). Future streaming work remains relevant, but only after Codex latency is under control or we have a parallel plan to reduce Codex response times.
3. **Next decisions:**
   - Investigate Codex latency (persistent PTY vs. CLI fallback, API responsiveness).
   - Decide whether to defer Phase 2B, pursue Codex-side improvements first, or shift to a faster Codex backend before investing 5–6 weeks in streaming Whisper.

All measurements were taken on 2025‑11‑13 and should be repeated once Codex latency improves to confirm whether voice becomes the new bottleneck.
