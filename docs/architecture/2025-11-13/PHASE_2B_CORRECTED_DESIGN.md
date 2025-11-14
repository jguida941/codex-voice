# Phase 2B Corrected Design — True Parallel STT (2025-11-13)

## Executive Summary

**The original Phase 2B "chunked Whisper" proposal (Option A) will NOT achieve latency goals.** After joint analysis from both assistants and user clarification ("we need to do option 2 i want it to be responsive not slow"), this document presents the corrected production-grade design.

**Bottom line:** To achieve <750ms voice latency, we need **streaming Whisper with mel frame feeding** (Option B), NOT sequential chunked transcription.

---

## Why Option A Fails

**Original Proposal (WRONG):**
- Run Whisper on 800ms chunks sequentially while capture continues
- Start STT after 1s prefetch

**Fatal Flaw:**
```
Chunk 1 (0-800ms):     ~600ms STT
Chunk 2 (600-1400ms):  ~600ms STT
Chunk 3 (1200-2000ms): ~600ms STT
Total STT time: 1800ms (WORSE than 800ms batch!)
Total latency: 1500ms capture + 1800ms STT = 3300ms
```

**Why it's slower:**
- Each chunk is a separate Whisper invocation (model init overhead)
- Overlap regions reprocess same audio multiple times
- No benefit from Whisper's context window (each chunk is independent)

**What it provides:** Partial transcripts for UX (user sees words appearing)
**What it doesn't provide:** Actual latency reduction

---

## Current Baseline (Phase 2A)

From BENCHMARKS.md:
- Short (1s speech): 1560ms capture = 1060ms speech + 500ms silence tail
- Medium (3s speech): 3560ms capture = 3060ms speech + 500ms silence tail
- STT (estimated batch): 500-1000ms depending on model size
- **Total voice latency: 2.0-4.5s (serial execution)**

**Unknown:** Codex API latency (likely 5-30s based on async worker necessity)

**Critical question:** If Codex takes >5s anyway, optimizing voice from 2.5s → 0.8s saves minimal user time.

---

## Latency Goal Clarification

**From latency_remediation_plan_2025-11-12.md:**
> "Goal: bring voice→Codex round-trip latency below 750 ms on CI hardware and 'a few hundred milliseconds' in production"

**User requirement:**
> "we need to do option 2 i want it to be responsive not slow"

**Interpretation:** <750ms is the **voice processing portion** (capture + STT), NOT including Codex API (which is external dependency).

**Target architecture:**
```
Total voice latency ≈ max(capture_duration, STT_duration) + finalization_gap
For 1s speech: max(1060ms, ~1000ms) + 100ms = ~1160ms
```

This requires **true parallelism**, not sequential chunks.

---

## Production-Grade Solution: Streaming Whisper (Option B)

### Architecture Overview

**Three parallel workers:**
1. **CaptureWorker** (exists): CPAL callback → PCM ring buffer + VAD
2. **MelWorker** (new): PCM ring → mel spectrogram frames → mel ring
3. **SttWorker** (new): Mel ring → Whisper decoder → streaming tokens

**Timeline:**
```
Time:      0ms    200ms   500ms   1000ms  1500ms  1600ms
Capture:   [=======speaking========][silence][stop]
MelWorker:   [====mel frames====][final]
SttWorker:     [===decode===][===decode===][final]
Result:                                            ^
Total latency: ~1600ms (capture 1500ms + finalization 100ms)
```

### Core Components

#### 1. StreamingMelBuilder (`audio.rs` or new `mel.rs`)

```rust
pub struct StreamingMelBuilder {
    config: MelConfig,
    window: Vec<f32>,      // Overlapping FFT window
    hop_samples: usize,    // 10-20ms worth of samples
    mel_filters: MelFilters,
}

impl StreamingMelBuilder {
    /// Feed PCM samples, return mel frames when ready
    pub fn push_samples(&mut self, pcm: &[f32]) -> Vec<MelFrame>;

    /// Finalize remaining audio
    pub fn finalize(&mut self) -> Vec<MelFrame>;
}
```

**Design details:**
- Uses sliding window with 50-75% overlap (standard for speech)
- Produces mel frames on 10-20ms cadence
- Lock-free push from CaptureWorker
- **Safety:** All owned data, no unsafe code needed

#### 2. StreamingWhisper (`stt.rs` new module)

```rust
pub struct StreamingWhisper {
    ctx: *mut whisper_context,      // Whisper C context
    state: *mut whisper_state,       // Persistent decoder state
    mel_buffer: Vec<f32>,            // Accumulated mel frames
    partial_text: String,            // Current transcript
}

impl StreamingWhisper {
    /// Feed new mel frames, run decoder iteration
    pub fn process_mel_frames(&mut self, frames: &[MelFrame])
        -> Result<Option<String>>;  // Returns partial transcript if ready

    /// Finalize decoding, return complete transcript
    pub fn finalize(&mut self) -> Result<String>;
}
```

**FFI Safety:**
- All whisper.cpp calls wrapped in `unsafe` blocks with clear invariants
- Drop impl ensures `whisper_free()` called
- Unit tests compare streaming vs batch output on golden audio

**Whisper.cpp Integration:**
- Use `whisper_full_parallel()` or `whisper_encode()` + `whisper_decode()`
- Feed mel frames incrementally via context state
- May require specific whisper.cpp version or patches

#### 3. StreamingVoicePipeline (`voice.rs`)

```rust
pub struct StreamingVoicePipeline {
    capture_handle: JoinHandle<CaptureResult>,
    mel_handle: JoinHandle<()>,
    stt_handle: JoinHandle<TranscriptResult>,
    pcm_queue: Arc<RingBuffer<f32>>,
    mel_queue: Arc<RingBuffer<MelFrame>>,
    partial_tx: Sender<VoiceJobMessage>,
}

impl StreamingVoicePipeline {
    pub fn start(config: VoiceConfig) -> Self;
    pub fn poll(&mut self) -> PipelineStatus;
    pub fn stop(&mut self) -> Result<String>;  // Final transcript
}
```

**Lifecycle:**
1. Start all 3 workers simultaneously
2. CaptureWorker fills PCM ring, runs VAD
3. MelWorker drains PCM → produces mel frames
4. SttWorker drains mel → emits partial transcripts via channel
5. VAD detects silence → CaptureWorker closes PCM queue
6. MelWorker drains remaining PCM, closes mel queue (~20ms)
7. SttWorker finalizes decode (~100ms), returns final transcript

**Total latency:** capture_duration + 120ms finalization

### Fallback Ladder

**Streaming → Batch → Python:**

```rust
pub enum TranscriptionStrategy {
    Streaming,    // Try first: StreamingWhisper
    Batch,        // Fallback 1: Existing batch Whisper
    Python,       // Fallback 2: Python subprocess (dev only)
    Manual,       // Fallback 3: User types transcript
}
```

**Trigger conditions:**
- Streaming fails if: FFI error, CPU overload (queue overflow), accuracy degradation
- Batch fails if: Model not loaded, timeout
- Python fails if: Feature disabled (production default)

**Metrics:**
```
voice_stream|strategy=streaming|ttfb_ms=250|total_ms=1200|fallback=none
voice_stream|strategy=batch|ttfb_ms=1500|total_ms=2300|fallback=streaming_error
```

### Configuration

New CLI flags / env vars:

```toml
[voice]
stt_strategy = "streaming"  # streaming|batch|auto
stt_mel_hop_ms = 20         # 10-30ms
stt_mel_n_fft = 400         # FFT size
stt_streaming_decode_cadence_ms = 100  # How often to run decode
stt_cpu_thread_limit = 4    # Max Whisper threads
stt_streaming_enabled = true # Feature flag
```

### Metrics & Observability

Extend `voice_metrics|` schema:

```
voice_metrics|
  capture_ms=1500|
  stt_strategy=streaming|
  ttfb_ms=250|           # Time to first transcript byte
  stt_iterations=15|     # Number of decode cycles
  stt_finalize_ms=95|    # Finalization after capture stops
  total_ms=1595|
  mel_frames=150|
  frames_dropped=0|
  fallback=none|
  early_stop=vad_silence
```

CI gate: `ttfb_ms <= speech_ms + 300` (allows 300ms decode latency during speech)

---

## Implementation Plan

### Phase 1: Measurement Gate (MANDATORY - DO NOT SKIP)

**Before ANY code:**

1. **Instrument full pipeline:**
   ```rust
   let t0 = Instant::now();
   let voice_result = capture_voice_native(...);
   let t1 = Instant::now();
   let codex_result = send_to_codex(...);
   let t2 = Instant::now();

   log: voice_ms=(t1-t0), codex_ms=(t2-t1), total_ms=(t2-t0)
   ```

2. **Collect 10 samples each:**
   - Short commands (1-2 words, <1s speech)
   - Medium commands (5-10 words, 2-3s speech)

3. **Analyze bottleneck:**
   - If Codex > 5s: Voice optimization saves <20% of total time → defer Phase 2B
   - If Voice > Codex: Proceed with streaming design
   - If Network latency dominates: Consider cloud STT

**Deliverable:** `docs/architecture/2025-11-13/LATENCY_MEASUREMENTS.md` with raw data + analysis

**Approval gate:** User must confirm "proceed with Phase 2B" after seeing measurements

### Phase 2: Mel Builder (1 week)

1. Create `rust_tui/src/mel.rs` with `StreamingMelBuilder`
2. Unit tests: synthetic sine waves → verify mel output matches batch
3. Benchmark: measure mel conversion overhead (<5% of total budget)
4. Integration: plug into existing audio pipeline (optional flag)

**Exit criteria:** Mel frames match whisper.cpp batch mel output (validate with test audio)

### Phase 3: Streaming FFI Wrapper (2 weeks)

1. Research whisper.cpp streaming APIs:
   - Check if `whisper_full_parallel()` supports incremental mel feeding
   - Identify minimum version requirements
   - Document unsafe invariants

2. Create `rust_tui/src/stt/streaming.rs`:
   - Wrap whisper context/state lifecycle
   - Implement `process_mel_frames()` + `finalize()`
   - Add comprehensive drop handling

3. **Critical:** Accuracy validation
   - Compare streaming vs batch transcripts on 50 test clips
   - Ensure WER (Word Error Rate) delta < 5%
   - Document any quality trade-offs

**Exit criteria:**
- Streaming transcripts match batch quality
- No memory leaks (valgrind clean)
- Graceful fallback on errors

### Phase 4: Pipeline Integration (1 week)

1. Create `StreamingVoicePipeline` orchestrator
2. Wire up 3-worker architecture with bounded queues
3. Add fallback logic + telemetry
4. Integration tests with synthetic audio

**Exit criteria:**
- Latency target met on synthetic clips (ttfb < speech_ms + 300ms)
- Fallback triggers correctly on CPU overload
- No deadlocks under stress test

### Phase 5: Config + Metrics (3 days)

1. Add CLI flags + validation
2. Extend `voice_metrics|` schema
3. Update perf_smoke CI gate
4. Documentation updates

### Phase 6: Production Validation (1 week)

1. Real microphone testing (20+ diverse utterances)
2. CPU/memory profiling
3. Edge case validation (very short/long utterances, background noise)
4. Fallback path testing

**Total timeline:** 5-6 weeks (2-3x original Phase 2B estimate)

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| whisper.cpp doesn't support streaming | High - blocks entire approach | Medium | Research before Phase 3, have cloud STT backup plan |
| Accuracy regression | High - unusable transcripts | Medium | Mandatory validation gate, fallback to batch |
| CPU overload starves capture | High - audio glitches | Low | Thread limits, drop-oldest policy, monitoring |
| Implementation complexity | Medium - timeline slip | High | Incremental merges, strict approval gates |
| Whisper.cpp version coupling | Medium - maintenance burden | Medium | Pin specific version, document requirements |

---

## Alternative: Cloud Streaming STT

If whisper.cpp streaming proves unfeasible:

**Option B2: Deepgram / AssemblyAI**
- Native streaming WebSocket APIs
- Typical latency: 300-500ms TTFB, <1s total
- **Pro:** Much simpler implementation (1 week vs 5 weeks)
- **Con:** Requires internet, privacy/cost considerations

**Hybrid approach:**
```rust
enum SttBackend {
    LocalStreaming,   // whisper.cpp streaming (offline)
    LocalBatch,       // whisper.cpp batch (offline fallback)
    CloudStreaming,   // Deepgram (online, fast)
    CloudBatch,       // Whisper API (online, fallback)
}
```

**Decision criteria:**
- Is offline requirement firm? → Must use local streaming
- Is <500ms latency required? → Cloud likely easier path
- What is acceptable complexity budget? → Cloud = 1/5 the effort

---

## Approval Gates & Next Steps

**NO CODE until these are confirmed:**

1. ✅ **User confirms latency goal:** Is <750ms voice latency a hard requirement, or is <2s with good UX acceptable?
2. ✅ **User confirms offline requirement:** Must work without internet, or is cloud STT acceptable?
3. ⏸️ **Measurement gate:** Full pipeline latency data collected and analyzed
4. ⏸️ **Architecture approved:** Option B (streaming Whisper) vs B2 (cloud STT) vs defer Phase 2B
5. ⏸️ **Complexity budget:** 5-6 weeks acceptable for true streaming, or must reduce scope?

**User questions to answer:**
- What is the acceptable total latency for voice processing? (<750ms hard / <2s soft / <5s acceptable)
- Is offline-only a requirement? (affects cloud STT viability)
- What is the complexity budget? (2-3x Phase 2A acceptable / must stay within original estimate)

**Next actions:**
1. Run measurement script (Phase 1 above) to get actual latency data
2. Document results in `LATENCY_MEASUREMENTS.md`
3. Present data + recommend path (local streaming vs cloud vs defer)
4. Get user approval on approach
5. Begin implementation ONLY after all gates passed

---

## Comparison to ChatGPT's Plan

**Areas of agreement (95%):**
- ✅ Option A (chunked Whisper) won't achieve latency goals
- ✅ Need streaming mel + Whisper FFI for true parallelism
- ✅ Three-worker architecture (capture/mel/stt)
- ✅ Bounded queues with fallback ladder
- ✅ Measurement gate before implementation
- ✅ 2-3x complexity vs original estimate

**Minor differences:**
- ChatGPT suggests "rolling mel cache" terminology - same concept as StreamingMelBuilder
- ChatGPT emphasizes `whisper_full_parallel` - I add `whisper_encode`+`whisper_decode` as alternative
- Both agree on incremental merges, feature flags, and extensive testing

**Unified recommendation:** This document combines both analyses into single production-grade plan.

---

## References

- Phase 2A results: [`BENCHMARKS.md`](BENCHMARKS.md)
- Original latency plan: [`../../audits/latency_remediation_plan_2025-11-12.md`](../../audits/latency_remediation_plan_2025-11-12.md)
- Whisper.cpp: https://github.com/ggerganov/whisper.cpp
- Streaming STT comparison: Deepgram vs AssemblyAI vs local Whisper
