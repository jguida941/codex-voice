# Changelog

## 2025-11-13
- Documented and implemented fail-fast PTY remediation (see `docs/architecture/2025-11-13/`), including `pty_disabled` runtime flag, responsive probe, and 150 ms / 500 ms timeouts.
- Added UI redraw macro, immediate input clearing, and PTY disable propagation via `CodexJobMessage::disable_pty`.
- Throttled high-quality audio resampler logs (single warning per process) and relaxed rubato length tolerance for cross-platform stability.
- Restored the `Ordering` import under all relevant cfgs and removed the duplicate `#![cfg(feature = "vad_earshot")]` in `vad_earshot.rs`, so `cargo clippy --all-features` and `cargo test --no-default-features` both succeed again.
- Established project traceability docs (`PROJECT_OVERVIEW.md`, `master_index.md`) and logged verification via `cargo fmt`, `cargo test --all`, and `cargo build --release`.
- Captured the Codex backend + PTY integration addendum (job IDs, bounded queues, recoverable/fatal error events, working-dir resolver) in `docs/architecture/2025-11-13/ARCHITECTURE.md` so the UI/voice layers can target a single backend contract.
- Replaced the legacy `CodexJob` worker with the new `CodexBackend` surface (`CliBackend`, bounded event queues, backend-owned PTY state) and updated `App`/tests to consume streaming `BackendEventKind` messages; verified via `cargo fmt` and `cargo test --no-default-features`.
