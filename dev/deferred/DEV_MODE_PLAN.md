# VoiceTerm Dev Mode - Design Document

## Overview

A separate developer overlay panel activated by `--dev-mode` flag that provides real-time audio analytics, session statistics, and conversation logging for developers building voice applications.

## Goals

- Provide an opt-in devtools overlay plus an offline analysis tool that share the same data model.
- Capture audio/stt metrics, session stats, and transcripts with enough metadata to debug latency, quality, and errors.
- Support fuzzy search, filters, tags, and bookmarks across sessions without changing the main HUD flow.
- Keep data collection and storage modular and UI-agnostic to avoid one-off "mode" files.
- Keep overhead near-zero when dev mode is disabled (no background logging or extra allocations).

## Non-goals

- No cloud sync, telemetry, or remote upload.
- No default audio capture storage; audio saving is an explicit opt-in.
- Not a replacement for the primary HUD or a production analytics platform.

## Privacy & Safety

- Dev logging is OFF by default. `--dev-mode` enables the overlay only; `--dev-log` enables on-disk logs.
- Provide retention controls (max days / max sessions) and a one-shot purge command.
- Add optional redaction rules (e.g. emails, api keys) before writing to disk.
- Store data in a dedicated dev folder with a schema version and clear filenames.

## Activation

```bash
voiceterm --dev-mode
# or
voiceterm -D

# proposed logging + storage controls
voiceterm --dev-mode --dev-log
voiceterm --dev-mode --dev-log --dev-path ~/.voiceterm/dev

# proposed offline analysis entrypoint
voiceterm dev
```

When active, a dedicated dev panel appears (toggleable with `Ctrl+D`) that shows real-time metrics without interfering with the main HUD. Logging stays in-memory unless `--dev-log` is set.

## Dev Data Tool (Offline Analysis)

A separate CLI/TUI for exploring saved sessions without running live capture. It reuses the same devtools modules and data schema as the overlay.

Planned capabilities:
- Session browser with summary stats and trend charts.
- Fuzzy search across transcripts with filters (time range, tags, latency, error type).
- Compare sessions or export slices for debugging regressions.

## Modularization Strategy

- Core data pipeline lives in `src/src/devtools/` (events, storage, search, stats).
- UI components consume `DevModeStats` via a small interface so overlay and offline tool share the same renderer.
- `voiceterm` overlay only wires events + toggles; avoid one-off `dev_mode.rs` / `dev_panel.rs` files in the bin.

---

## Panel Layout

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ VoiceTerm DevTools                                        [×] ^D to close   ┃
┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃                                                                           ┃
┃  ┌─ Audio Quality ─────────────┐  ┌─ Session ─────────────────────────┐   ┃
┃  │ SNR      ████████░░  24dB   │  │ Uptime      1h 23m 45s            │   ┃
┃  │ Peak     -8dB (hold: -4dB)  │  │ Transcripts 42                    │   ┃
┃  │ RMS      -14dB              │  │ Words       1,247                 │   ┃
┃  │ Crest    6dB                │  │ Avg Latency 320ms                 │   ┃
┃  │ Floor    -52dB              │  │ Errors      0                     │   ┃
┃  │ Clip     None               │  │ Audio Time  12m 34s               │   ┃
┃  └─────────────────────────────┘  └───────────────────────────────────┘   ┃
┃                                                                           ┃
┃  ┌─ Waveform ──────────────────────────────────────────────────────────┐  ┃
┃  │ ▁▂▃▄▅▆▇█▇▆▅▄▃▂▁▁▂▃▄▅▆▇█▇▆▅▄▃▂▁▁▂▃▅▆▇█▇▆▅▄▃▂▁▁▂▃▄▅▆▇█▇▆▅▄▃▂▁        │  ┃
┃  └─────────────────────────────────────────────────────────────────────┘  ┃
┃                                                                           ┃
┃  ┌─ STT Service ───────────────┐  ┌─ Last Transcript ─────────────────┐   ┃
┃  │ Provider  Whisper API       │  │ "Hello this is a test of the..."  │   ┃
┃  │ Status    ● Connected       │  │ Confidence: 94%  Latency: 342ms   │   ┃
┃  │ Queue     0 pending         │  │ Words: 12  Duration: 3.2s         │   ┃
┃  │ Retries   0                 │  │ WPM: 225                          │   ┃
┃  └─────────────────────────────┘  └───────────────────────────────────┘   ┃
┃                                                                           ┃
┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃ [S]ave  [E]xport  [/]Search  [F]ilter  [T]ag  [B]ookmark  [C]lear  Log: ON ● ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

---

## Metrics Definitions

### Audio Quality Section

| Metric | Description | Formula/Source |
|--------|-------------|----------------|
| **SNR** | Signal-to-Noise Ratio | `signal_rms_db - noise_floor_db` |
| **Peak** | Maximum amplitude this recording | `20 * log10(max_sample)` |
| **Peak Hold** | Highest peak in session | Persists until reset |
| **RMS** | Root Mean Square (average loudness) | `20 * log10(sqrt(mean(samples²)))` |
| **Crest** | Dynamic range indicator | `peak_db - rms_db` |
| **Floor** | Ambient noise level | Measured during silence |
| **Clip** | Clipping events detected | Count of samples >= 0.99 |

### Session Statistics

| Metric | Description |
|--------|-------------|
| **Uptime** | Time since voiceterm started |
| **Transcripts** | Total successful transcriptions |
| **Words** | Total words transcribed |
| **Avg Latency** | Mean transcription latency |
| **Errors** | Failed transcription attempts |
| **Audio Time** | Total recording duration |

### Latency Breakdown (per transcript)

| Metric | Description |
|--------|-------------|
| **Capture** | Mic capture + buffering time |
| **VAD** | Voice activity detection time |
| **Encode** | Resample/encode time |
| **Network** | Request/response transit time |
| **STT** | Model decode time |
| **Post** | Post-processing time (normalize/punctuate) |

### Storage & Index

| Metric | Description |
|--------|-------------|
| **Log Size** | Bytes written this session |
| **Events** | Total dev events logged |
| **Index Size** | Search index size on disk |
| **Retention** | Oldest session kept |

### STT Service

| Metric | Description |
|--------|-------------|
| **Provider** | Whisper API / Local / Python |
| **Status** | Connected / Disconnected / Error |
| **Queue** | Pending transcription jobs |
| **Retries** | Retry attempts this session |

### Per-Transcript

| Metric | Description |
|--------|-------------|
| **Confidence** | STT confidence score (if available) |
| **Latency** | Processing time for this transcript |
| **Words** | Word count |
| **Duration** | Audio duration |
| **WPM** | Words per minute |

---

## Data Structures

```rust
/// Dev mode configuration (runtime + storage)
pub struct DevModeConfig {
    pub enabled: bool,
    pub log_to_disk: bool,
    pub data_dir: PathBuf,
    pub retention_days: u32,
    pub redact_rules: Vec<RedactRule>,
}

/// Dev mode session statistics
pub struct DevModeStats {
    pub session_id: String,
    pub started_at: Instant,
    pub transcript_count: u32,
    pub word_count: u32,
    pub latency_sum_ms: u64,
    pub error_count: u32,
    pub audio_duration_ms: u64,
    pub peak_hold_db: f32,
    pub noise_floor_db: f32,
    pub clip_count: u32,
    pub transcripts: Vec<TranscriptRecord>,
    pub events: Vec<DevEvent>,
}

/// Individual transcript record
pub struct TranscriptRecord {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub text: String,
    pub normalized_text: String,
    pub tags: Vec<String>,
    pub bookmarked: bool,
    pub confidence: Option<f32>,
    pub latency_ms: u32,
    pub latency_breakdown: LatencyBreakdown,
    pub duration_ms: u32,
    pub word_count: u32,
    pub audio_peak_db: f32,
    pub audio_rms_db: f32,
    pub source: TranscriptSource,
}

pub struct LatencyBreakdown {
    pub capture_ms: u32,
    pub vad_ms: u32,
    pub encode_ms: u32,
    pub network_ms: u32,
    pub stt_ms: u32,
    pub post_ms: u32,
}

pub enum DevEvent {
    Transcript(TranscriptRecord),
    AudioMetrics(AudioMetrics),
    Error(DevError),
    Marker(DevMarker),
}

pub struct DevError {
    pub timestamp: DateTime<Utc>,
    pub kind: String,
    pub message: String,
}

pub struct DevMarker {
    pub timestamp: DateTime<Utc>,
    pub label: String,
}

/// Real-time audio metrics
pub struct AudioMetrics {
    pub snr_db: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    pub crest_db: f32,
    pub floor_db: f32,
    pub is_clipping: bool,
}
```

---

## Export Formats

### Session JSONL (events) (`~/.voiceterm/dev/sessions/session-2026-02-02-19h30.jsonl`)

Each line is a single `DevEvent` with a stable schema version.

```json
{"type":"transcript","ts":"2026-02-02T19:30:15Z","text":"Hello this is a test","latency_ms":342}
{"type":"audio_metrics","ts":"2026-02-02T19:30:15Z","snr_db":24.1,"rms_db":-14.1}
```

### Session CSV (summary) (`~/.voiceterm/dev/sessions/session-2026-02-02-19h30.summary.csv`)

```
timestamp,text,latency_ms,confidence,tags,bookmarked,duration_ms
2026-02-02T19:30:15Z,"Hello this is a test",342,0.94,"debug;smoke",false,3200
```

### Session Markdown (`~/.voiceterm/dev/sessions/session-2026-02-02-19h30.md`)

```markdown
---
session_id: voiceterm-2026-02-02-19h30
started: 2026-02-02T19:30:00Z
ended: 2026-02-02T20:53:45Z
duration_minutes: 83
transcript_count: 42
word_count: 1247
avg_latency_ms: 320
error_count: 0
audio_minutes: 12.5
peak_hold_db: -4.2
noise_floor_db: -52.3
---

# VoiceTerm Session - Feb 2, 2026

## Summary

| Metric | Value |
|--------|-------|
| Duration | 1h 23m |
| Transcripts | 42 |
| Words | 1,247 |
| Avg Latency | 320ms |
| Errors | 0 |
| Audio Time | 12m 30s |

## Audio Quality

- Peak Hold: -4.2 dB
- Noise Floor: -52.3 dB
- Clipping Events: 0

## Transcripts

### 1. 19:30:15 (342ms, 94% confidence)
> Hello, this is a test of the voice transcription system.

**Audio:** 3.2s duration, -8.2dB peak, -14.1dB RMS, 225 WPM

---

### 2. 19:31:42 (298ms, 97% confidence)
> Can you help me debug this function?

**Audio:** 2.1s duration, -10.4dB peak, -16.2dB RMS, 180 WPM

---

[... more transcripts ...]
```

### Session JSON (`~/.voiceterm/dev/sessions/session-2026-02-02-19h30.json`)

```json
{
  "session_id": "voiceterm-2026-02-02-19h30",
  "started": "2026-02-02T19:30:00Z",
  "ended": "2026-02-02T20:53:45Z",
  "stats": {
    "duration_seconds": 4995,
    "transcript_count": 42,
    "word_count": 1247,
    "avg_latency_ms": 320,
    "error_count": 0,
    "audio_seconds": 750
  },
  "audio_quality": {
    "peak_hold_db": -4.2,
    "noise_floor_db": -52.3,
    "clip_count": 0
  },
  "transcripts": [
    {
      "timestamp": "2026-02-02T19:30:15Z",
      "text": "Hello, this is a test of the voice transcription system.",
      "confidence": 0.94,
      "latency_ms": 342,
      "duration_ms": 3200,
      "word_count": 10,
      "peak_db": -8.2,
      "rms_db": -14.1
    }
  ]
}
```

---

## Implementation Plan

### Phase 0: Architecture + Modularization (Priority: High)
- [ ] Create `devtools` module in `src/src/devtools/` with clear boundaries (config, events, storage, search, ui).
- [ ] Define stable `DevEvent` schema + versioning.
- [ ] Add a thin bridge in `voiceterm` to avoid one-off "mode" files.

### Phase 1: Flags + Data Pipeline (Priority: High)
- [ ] Add `--dev-mode` / `-D` to enable overlay only.
- [ ] Add `--dev-log` / `--dev-path` to opt into on-disk logging.
- [ ] Emit `DevEvent` from audio + stt; keep an in-memory ring buffer for live UI.
- [ ] JSONL writer with backpressure and rotation.

### Phase 2: Dev Panel Overlay (Priority: Medium)
- [ ] Render live metrics from shared `DevModeStats`.
- [ ] Add footer controls for search, filter, tag, bookmark, export.
- [ ] Keep overlay toggle with `Ctrl+D` when dev mode is active.

### Phase 3: Search + Analysis (Priority: Medium)
- [ ] Build fuzzy search index on transcripts (in-memory + persisted index).
- [ ] Support filters (time range, latency threshold, error type, tags).
- [ ] Session list + compare view (diff key metrics).

### Phase 4: Offline Analysis Tool (Priority: Medium)
- [ ] `voiceterm dev` subcommand to browse saved sessions without live audio.
- [ ] Batch export commands for JSON/CSV/MD with filters.

### Phase 5: Export + Retention (Priority: Low)
- [ ] Auto-save on exit (when `--dev-log`).
- [ ] Retention policy and purge command.
- [ ] Optional audio clip storage (explicit opt-in).

---

## Files to Create/Modify

| File | Action | Purpose |
|------|--------|---------|
| `src/src/devtools/mod.rs` | CREATE | Devtools module entrypoint |
| `src/src/devtools/config.rs` | CREATE | Config + defaults + CLI mapping |
| `src/src/devtools/events.rs` | CREATE | DevEvent schema + versioning |
| `src/src/devtools/state.rs` | CREATE | Stats + aggregation |
| `src/src/devtools/storage.rs` | CREATE | JSONL writer + retention |
| `src/src/devtools/search.rs` | CREATE | Fuzzy search + filters |
| `src/src/devtools/panel.rs` | CREATE | Dev panel rendering (shared) |
| `src/src/devtools/export.rs` | CREATE | MD/JSON/CSV exporters |
| `src/bin/voiceterm/main.rs` | MODIFY | Wire devtools bridge + flags |
| `src/bin/voiceterm/config.rs` | MODIFY | CLI flags |
| `src/bin/voiceterm/input.rs` | MODIFY | Keybindings |
| `src/src/legacy_ui.rs` | MODIFY | Shared rendering hooks (if needed) |

---

## Open Questions

1. **Panel Position**: Full screen overlay or side panel?
2. **Offline Tool Name**: `voiceterm dev` vs `voiceterm analyze`?
3. **Search Implementation**: Simple fuzzy matcher vs full-text index?
4. **Storage Format**: JSONL only, or JSONL + SQLite for queries?
5. **Audio Storage**: Should short audio clips be supported (explicit opt-in)?
6. **Retention Defaults**: What is a safe default policy for dev logs?
7. **Schema Versioning**: How strict do we want backward compatibility?

---

## References

- [Ratatui TUI Framework](https://ratatui.rs/)
- [Audio Crest Factor](https://www.izotope.com/en/learn/what-is-crest-factor)
- [Claude Code Memory](https://code.claude.com/docs/en/memory)
- [Signal-to-Noise Ratio](https://en.wikipedia.org/wiki/Signal-to-noise_ratio)
