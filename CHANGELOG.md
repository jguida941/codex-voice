# Changelog

All notable changes to this project will be documented here, following the SDLC policy defined in `agents.md`.

## [Unreleased]

### CRITICAL - Phase 2B Design Correction (2025-11-13 Evening)
- **Rejected original Phase 2B "chunked Whisper" proposal (Option A)** after identifying fatal architectural flaw: sequential chunk transcription provides NO latency improvement (capture + Σchunks often slower than capture + single_batch). Original proposal would have wasted weeks implementing slower approach.
- **Documented corrected design** in `docs/architecture/2025-11-13/PHASE_2B_CORRECTED_DESIGN.md` specifying streaming mel + Whisper FFI architecture (Option B) as only viable path to <750ms voice latency target. User confirmed requirement: "we need to do option 2 i want it to be responsive not slow".
- **Design includes:** Three-worker parallel architecture (capture/mel/stt), streaming Whisper FFI wrapper, fallback ladder (streaming→batch→Python), comprehensive measurement gates, 5-6 week implementation plan, and mandatory approval gates before any coding begins.
- **Measurement gate executed:** Recorded 10 end-to-end Ctrl+R→Codex runs (short/medium commands) and captured results in `docs/architecture/2025-11-13/LATENCY_MEASUREMENTS.md`. Voice pipeline averages 1.19 s, Codex averages 9.2 s (≈88 % of total latency), so Phase 2B remains blocked until Codex latency is addressed or stakeholders explicitly accept the limited ROI.
- **Codex remediation plan:** Authored `docs/architecture/2025-11-13/CODEX_LATENCY_PLAN.md` covering telemetry upgrades, PTY/CLI health checks, CLI profiling, and alternative backend options. Phase 2B remains gated on executing this plan and obtaining stakeholder approval.
- **Persistent PTY fix:** Health check now waits 5 s and only verifies the child process is alive (no synthetic prompts), so conversations start clean and persistent sessions stay responsive. Python helper path was removed; persistent mode is pure Rust. Next Codex task is streaming output so the UI shows tokens as they arrive.
- **Next steps:** (1) Address Codex latency or approve proceeding despite the bottleneck, (2) Decide between local streaming (Option B) vs. cloud STT vs. deferral, (3) Confirm complexity budget (5–6 weeks acceptable), (4) Only after approvals resume Phase 2B work.

### Added
- Completed the Phase 2A recorder work: `FrameAccumulator` maintains bounded frame buffers with lookback-aware trimming, `CaptureMetrics` now report `capture_ms`, and perf smoke parses the real `voice_metrics|…` log lines emitted by `voice.rs`.
- Added `CaptureState` helpers plus unit tests covering max-duration timeout, min-speech gating, and manual stop semantics so recorder edge cases stay regression-tested.
- Phase 2A scaffolding: introduced the `VadEngine` trait, Earshot feature gating, and a fallback energy-based VAD so recorder callers can swap implementations without API churn.
- Runtime-selectable VADs: new `--voice-vad-engine <earshot|simple>` flag (documented in `docs/references/quick_start.md`), validation, and `VoicePipelineConfig` plumbing so operators can pick Earshot (default when the feature is built) or the lightweight threshold fallback without touching source code.
- Added the `vad_earshot` optional dependency/feature wiring in `Cargo.toml` together with the new `rust_tui/src/vad_earshot.rs` adapter.
- Updated the voice pipeline to call `Recorder::record_with_vad`, log per-utterance metrics, and honor the latency plan’s logging/backpressure rules.
- Introduced the async Codex worker module (`rust_tui/src/codex.rs`) plus the supporting test-only job hook harness so Codex calls can run off the UI thread and remain unit-testable without shelling out.
- Documented the approved Phase 1 backend plan (“Option 2.5” event-stream refactor) in `docs/architecture/2025-11-13/ARCHITECTURE.md`, capturing the Wrapper Scope Correction + Instruction blocks required before touching Codex integration.
- Added perf/memory guard rails: `app::tests::{perf_smoke_emits_timing_log,memory_guard_backend_threads_drop}` together with `.github/workflows/perf_smoke.yml` and `.github/workflows/memory_guard.yml` so CI enforces telemetry output and backend thread cleanup.
- Implemented the Phase 1 backend refactor: `CodexBackend`/`BackendJob` abstractions with bounded queues, `CliBackend` PTY ownership, App wiring to backend events, and new queue/tests (`cargo test --no-default-features`).
- Benchmark harness for Phase 2A: `audio::offline_capture_from_pcm`, the `voice_benchmark` binary, `scripts/benchmark_voice.sh`, and `docs/architecture/2025-11-13/BENCHMARKS.md` capture deterministic short/medium/long clip metrics that feed the capture_ms SLA.

### Changed
- Replaced the `Recorder::record_with_vad` stub (non-test builds) with the new chunked capture loop (bounded channel + VAD decisions + metrics) ahead of the perf_smoke gate.
- `App`/`ui` now spawn Codex work asynchronously, render a spinner with Esc/Ctrl+C cancellation, and log `timing|phase=codex_job|...` metrics; `cargo test --no-default-features` gates the new worker while the `earshot` crate remains offline.
- Corrected the Earshot profile mapping (`rust_tui/src/vad_earshot.rs`) and fixed the Rubato `SincFixedIn` construction (`rust_tui/src/audio.rs`) so the high-quality resampler runs cleanly instead of spamming “expected 256 channels” and falling back on every frame.
- Introduced an explicit redraw flag in `App` plus a simplified `ui.rs` loop so job completions and spinner ticks refresh the TUI automatically; recordings/transcripts now appear without requiring stray keypresses while still capping idle redraws.
- Tightened the persistent Codex PTY timeout (2 s to first printable output, 10 s overall) so we fall back to the CLI path quickly when the helper is silent, eliminating the 30–45 s per-request stall.
- Resolved the remaining clippy warnings by introducing `JobContext`, simplifying queue errors, modernizing format strings, and gating unused imports; `cargo clippy --no-default-features` now runs clean.

### Fixed
- Restored the missing atomic `Ordering` import under all feature combinations (`rust_tui/src/audio.rs`) and removed the redundant crate-level cfg guard from `rust_tui/src/vad_earshot.rs`, unblocking `cargo clippy --all-features` and `cargo test --no-default-features`.
- Codex backend module once again satisfies `cargo fmt`: moved the `#[cfg(test)]` attribute ahead of the gated `AtomicUsize` import (`rust_tui/src/codex.rs`) to follow Rust formatting rules.
- GitHub Actions Linux runners now install ALSA headers before running our audio-heavy tests (`.github/workflows/perf_smoke.yml`, `.github/workflows/memory_guard.yml`), fixing the `alsa-sys` build failures on CI.
- `voice_benchmark` now validates `--voice-vad-engine earshot` against the `vad_earshot` feature via the new `ensure_vad_engine_supported` helper plus clap-based unit tests, preventing the `unreachable!()` panic the reviewer observed when the benchmark binary is compiled without the feature flag.

### Known Issues
- `cargo check`/`test` cannot download the `earshot` crate in this environment; run the builds once network access is available to validate the new code paths.

## [2025-11-12]
### Added
- Established baseline governance artifacts: `master_index.md`, repository `CHANGELOG.md`, root `PROJECT_OVERVIEW.md` (planned updates), and the initial dated architecture folder under `docs/architecture/`.
- Consolidated legacy documentation into the daily architecture tree (`docs/architecture/YYYY-MM-DD/`), `docs/references/` (formerly `docs/guides/`), and `docs/audits/`; backfilled the original architecture overview into `docs/architecture/2025-11-11/`.
- Relocated root-level guides (`ARCHITECTURE.md`, `MASTER_DOC.md`, `plan.md`) into `docs/references/`, corrected the historical architecture baseline to `docs/architecture/2025-11-11/`, and updated navigation pointers accordingly.
- Updated the new references (`quick_start.md`, `testing.md`, `python_legacy.md`) to reflect the current Rust pipeline (Ctrl+R voice key, `cargo run` workflow, native audio tests) and annotated the legacy plan in `docs/archive/MVP_PLAN_2024.md`.
- Added a concise root `README.md`, introduced the “You Are Here” section in `PROJECT_OVERVIEW.md`, renamed `docs/guides/` → `docs/references/` (`quick_start.md`, `testing.md`, `python_legacy.md`, `milestones.md`, `troubleshooting.md`), and archived superseded guides under `docs/archive/OBSOLETE_GUIDES_2025-11-12/`.
- Updated helper scripts (`rust_tui/test_performance.sh`, `test_voice.sh`, `simple_test.sh`, `final_test.sh`) to rely on `cargo run`, Ctrl+R instructions, and the shared `${TMPDIR}/codex_voice_tui.log`.
- Extended `agents.md` with an end-of-session checklist so every workday records architecture notes, changelog entries, and the “You Are Here” pointer.
- Consolidated the CI/CD references into a single `docs/references/cicd_plan.md`, merging the previous implementation + dependency guides and archiving the superseded files under `docs/archive/OBSOLETE_REFERENCES_2025-11-12/`.
- Expanded `docs/references/cicd_plan.md` with appendices covering phase-by-phase scripts, tooling/dependency matrices, rollback/cost controls, and troubleshooting so it fully supersedes the archived references.
- Captured the latency remediation plan in `docs/audits/latency_remediation_plan_2025-11-12.md` and updated `PROJECT_OVERVIEW.md` to prioritize latency stabilization workstreams ahead of module decomposition and CI enhancements.
- Strengthened the latency plan with explicit Phase 2A/2B/3 naming, backpressure/frame/shutdown/fallback policies, and structured tracing + CI perf gate requirements so phase execution is unambiguous.
- Added production-grade detail to the latency plan: failure hierarchy, VAD safety rails, bounded resource budgets, observability schema, CI enforcement hooks, and a 15-day execution timeline.
- Updated `agents.md` so the latency requirements explicitly point to the Phase 2B specification (`docs/audits/latency_remediation_plan_2025-11-12.md`) and mandate adherence to the new resource/observability/CI rules.
- Documented the state machine, config/deployment profiles, and concurrency guardrails inside the latency plan so downstream work follows the same lifecycle semantics.
- Hardened `agents.md` with Scope/Non-goals, "Before You Start" instructions, condensed voice requirements referencing the latency plan, explicit doc-update rules, and a prominent end-of-session checklist.
- Recorded the readiness audit (`docs/audits/READINESS_AUDIT_2025-11-12.md`), summarized its findings in the architecture log, and captured the Phase 2A design (Earshot VAD, config surface, metrics, exit criteria) plus the immediate task list (perf_smoke CI, Python feature flag, module decomposition planning).


See `docs/audits/latency_remediation_plan_2025-11-12.md` for the complete latency specification.
