# Project Overview

## 🎯 You Are Here

- **Current Session**: 2025-11-13
- **Latest Notes**: [`docs/architecture/2025-11-13/`](docs/architecture/2025-11-13/)

**Today We Finished**
- **Phase 2A COMPLETE**: Exposed runtime VAD selector (`--voice-vad-engine earshot|simple`), created benchmark harness (`voice_benchmark` binary + `scripts/benchmark_voice.sh`), documented SLA evidence in `BENCHMARKS.md` (conservative targets: capture_ms ≤1.8s for short clips, ≤4.2s for <3s speech), added 6 VAD engine selection tests, and logged technical debt for Phase 2B. All exit criteria satisfied (CLI config surface, metrics, tests, CI scaffolding, documentation).
- Opened the 2025-11-13 daily folder and logged the formal Phase 2A kickoff plus Earshot VAD approval.
- Landed FrameAccumulator + `voice_metrics|` schema: recorder enforces lookback-aware trimming, `CaptureMetrics` includes `capture_ms`, perf_smoke parses structured logs, and 10 unit tests cover silence stop/drop-oldest/min-speech flows.
- Shipped async Codex worker (`codex.rs`, `App::poll_codex_job`) so TUI stays responsive during 30-60s Codex calls.
- Completed Phase 1A stabilization: clippy clean, perf/memory guard tests, CI workflows (`perf_smoke.yml`, `memory_guard.yml`).
- Completed Phase 1 backend refactor: `CodexBackend` trait + `CliBackend`, bounded event queues, drop-oldest policy.

**In Progress**
- **Phase 2B design review** — Streaming capture with overlapped STT (bounded SPSC queue, worker lifecycle, latency overlap) using Phase 2A benchmarks as baseline
- Module decomposition options for `app.rs`/`pty_session.rs` (Phase 1B pending Phase 2B completion)
- Monitoring perf/memory CI gates and updating perf smoke thresholds once Phase 2A SLA is promoted to CI enforcement

**Next Session**
1. **Draft Phase 2B design proposal** with alternatives (streaming capture handle, STT worker architecture, queue backpressure) for approval before implementation.
2. Add docs/changelog enforcement workflow so CI blocks code changes missing daily architecture folder + root changelog updates.
3. Validate Earshot against real microphone captures to complement synthetic benchmarks and finalize perf smoke thresholds.
4. Begin module decomposition (Phase 1B) once Phase 2B lands to reduce `app.rs`/`pty_session.rs`/`codex.rs`/`audio.rs` to ≤300 LOC targets.

---

Central roadmap for the Codex Voice-to-Codex wrapper. This document summarizes the current scope, major architectural decisions, and pointers into the dated architecture log.

## Mission
- Deliver a Rust-first voice interface that extends (not replaces) Codex, adding voice capture, standardized formatting, diagnostics, CI/testing harnesses, and future IDE-style tooling.
- Enforce the SDLC discipline defined in `agents.md`: design-first workflow, modular code (200–300 LOC per module), full traceability (CHANGELOG + daily architecture notes), and strong test/benchmark coverage.

## Current Focus
1. Stand up mandatory governance scaffolding (daily architecture folders, CHANGELOG, master index).
2. Re-architecture the voice pipeline to meet sub-second round-trip targets (silence-aware capture + overlapped STT).
3. Decompose oversized modules (e.g., `rust_tui/src/app.rs`) while preserving Codex integration surface.

## Architectural Decision Log
- Latest daily notes: [`docs/architecture/2025-11-13/`](docs/architecture/2025-11-13/)
- Previous day baseline: [`docs/architecture/2025-11-12/`](docs/architecture/2025-11-12/)
- Historical decisions: see `docs/architecture/` index (one folder per working day; each folder links back to the previous day to form a breadcrumb trail).

## Directory Map
See `master_index.md` for a detailed description of every top-level directory and governance file.

## Key Reference Docs
- `docs/references/cicd_plan.md` — Single-source CI/CD blueprint covering the pipeline architecture, dependencies, and the phased implementation guide.
- `docs/references/quick_start.md` / `testing.md` — Operational procedures for building, running, and validating the voice wrapper.

## Update Checklist
When making changes:
1. Draft design reasoning + alternatives in the current day's `docs/architecture/YYYY-MM-DD/ARCHITECTURE.md`.
2. Record user-approved decisions and benchmarks in the same folder.
3. Update this overview when major goals or decision links change.
4. Append the repository `CHANGELOG.md`.
5. Ensure CI sees both the daily architecture note and CHANGELOG updates.
