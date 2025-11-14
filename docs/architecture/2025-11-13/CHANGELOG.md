# Daily Changelog — 2025-11-13

## Added
- Logged the Phase 1 backend decision (“Option 2.5”) in `ARCHITECTURE.md`, including the full Wrapper Scope Correction + Instruction blocks to confirm SDLC alignment before coding.
- Created `.github/workflows/perf_smoke.yml` (timing log enforcement) and `.github/workflows/memory_guard.yml` (backend thread cleanup loop) so CI now checks the telemetry and worker-lifecycle gates mandated by the latency plan.
- Added `app::tests::perf_smoke_emits_timing_log` and `memory_guard_backend_threads_drop` plus the supporting backend thread counters so perf/memory guards can run deterministically in CI.
- Implemented the `CodexBackend` trait, `BackendJob` event queue, and `CliBackend` (bounded channel + drop-oldest policy) plus the refactored `run_codex_job` emitting `BackendEventKind` streams so the UI is now decoupled from PTY/CLI details.
- Created the 2025-11-13 architecture folder capturing the Phase 2A kickoff, Earshot approval, and success criteria checklist.
- Documented Phase 2A work plan (config surface, Earshot integration, metrics, tests, CI stubs) per the latency remediation plan.
- Implemented `VadEngine` interfaces in `rust_tui/src/audio.rs`, including the Earshot feature flag wiring and a `SimpleThresholdVad` fallback for non-Earshot builds.
- Added the `vad_earshot` feature and optional dependency placeholder in `rust_tui/Cargo.toml`.
- Introduced `rust_tui/src/vad_earshot.rs` (feature gated) plus the trait-based VAD factory used by `voice.rs`.
- Reworked `Recorder::record_with_vad` to prepare for chunked capture (non-test builds) and added per-utterance metrics scaffolding/logging.
- Updated `voice.rs` to call the new VAD-aware recorder path and emit capture metrics alongside transcripts.
- Captured the detailed Option 1 Codex worker design (state flow, cancellation, spinner plan, telemetry) in `ARCHITECTURE.md` so implementation can proceed under SDLC.
- Implemented the nonblocking Codex worker (`rust_tui/src/codex.rs` + `App::poll_codex_job`), spinner/cancel UX, and session handoff; TUI no longer freezes during 30–60 s Codex calls and cancellation now surfaces via Esc/Ctrl+C.
- Added unit tests for the worker success/error/cancel paths plus new UI-level tests that drive the spinner/cancel flow via the job hook harness; `cargo test --no-default-features` is now part of the daily verification until the `earshot` crate is reachable.
- Reworked the render loop (`rust_tui/src/ui.rs`) and `App` state (`needs_redraw`) so job completions and spinner ticks trigger redraws automatically, eliminating the “press any key to see output” behavior during voice capture or Codex runs.
- Shortened the persistent Codex PTY timeout to 10 s with a 2 s “first output” deadline (`rust_tui/src/codex.rs`) so we bail to the fast CLI path almost immediately when the helper isn’t producing printable output, fixing the 30–45 s stalls per request.
- Documented the Codex backend addendum (job IDs, bounded event queues, PTY fast-fail ownership, backend event mapping, telemetry fields, and regression hooks) inside `docs/architecture/2025-11-13/ARCHITECTURE.md` so the wrapper/voice layers share a single integration contract.

## Fixed
- Corrected the Earshot profile mapping (`rust_tui/src/vad_earshot.rs`) to use the actual `VoiceActivityProfile::QUALITY/LBR/AGGRESSIVE/VERY_AGGRESSIVE` constants so release builds succeed once the crate is available.
- Swapped the Rubato `SincFixedIn` constructor arguments (`rust_tui/src/audio.rs`) so chunk size and channel count are not inverted; this stops the "expected 256 channels" spam, keeps high-quality resampling enabled, and prevents runaway log growth during idle TUI sessions.
- **CRITICAL:** Fixed race condition in `App::poll_codex_job` (`app.rs:527-536`) where job was cleared before handling completion message, causing state inconsistency.
- **CRITICAL:** Changed atomic ordering to `AcqRel` for `RESAMPLER_WARNING_SHOWN` flag (`audio.rs:575`) to prevent data race in multi-threaded audio capture.
- **HIGH:** Improved `PtyCodexSession::is_responsive()` (`pty_session.rs:114-130`) to drain stale output 5 times before probing, preventing false positives from buffered data.
- **HIGH:** Fixed hardcoded 500ms timeout in PTY polling loop (`codex.rs:384`) to use proper 50ms interval for responsive detection within 150ms/500ms fail-fast limits.
- Cleared all `cargo clippy --no-default-features` warnings by introducing `JobContext`, simplifying `BackendQueueError`, modernizing format strings, gating imports per `cfg(test)`, and cleaning up `ui.rs`, `utf8_safe.rs`, `voice.rs`, `pty_session.rs`, and the `test_*` binaries.
- Ensured `cargo test --no-default-features` runs warning-free by gating unused imports and adding perf/memory guard tests that assert backend telemetry + thread counters behave as expected.
- `App::poll_codex_job` no longer races cleared jobs vs. final events; the backend event queue drains fully, joins worker threads deterministically, and avoids the prior “worker disconnected” false-positive statuses in tests.
- Restored Phase 1A build hygiene after the audit by importing `Ordering` under all relevant cfgs and deleting the duplicate `#![cfg(feature = "vad_earshot")]` attribute, then re-running `cargo clippy --all-features` and `cargo test --no-default-features` to confirm green builds.

## Pending
- Implementation of Earshot-based silence-aware capture and the accompanying metrics/tests.
- Addition of `perf_smoke` and `memory_guard` workflows tied to the new metrics.
- Manual testing of async Codex worker UI responsiveness and cancellation behavior once the reference environment is back online.

## Notes
- Future updates to this file must capture concrete code/doc changes completed on 2025-11-13. Document any backend refactor progress (trait definitions, adapter removal, event stream wiring) as soon as work lands.
- Verified `cargo fmt` + `cargo test --no-default-features` under `rust_tui/`; audio module warnings remain from pre-existing stubs and are tracked separately.
- `cargo clippy --no-default-features` now runs clean locally and in CI; perf/memory workflows should remain green before proceeding to module decomposition.
