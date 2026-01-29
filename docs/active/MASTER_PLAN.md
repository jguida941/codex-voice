# Master Plan (Active)

## Purpose
- Single source of truth for active work, audits, and verification.
- New items go here; completed items move to `docs/archive/` with a dated entry.

## References
- `agents.md` (SDLC policy and verification)
- `docs/dev/ARCHITECTURE.md`
- `docs/dev/DEVELOPMENT.md`
- `docs/dev/adr/`
- `docs/CHANGELOG.md`

## Source inputs
- `docs/archive/2026-01-29-claude-review.md`
- `docs/archive/2026-01-29-claude-audit.md`
- `docs/active/BACKLOG.md` (migrated here)

## Active work

### P0 - Reliability and UX correctness
- [ ] MP-001 Fix transcript queue flush reliability (user-reported).
- [ ] MP-002 Stop status spam: repeated "Transcript ready (Rust pipeline)" lines.
- [ ] MP-003 Investigate unexpected "Use /skills..." output in UI and confirm source.
- [ ] MP-004 Confirm auto-voice "Listening..." stays visible while capture is active; fix if not.

### P1 - Testing and verification
- [ ] MP-010 Add integration tests for voice -> STT -> injection flow.
- [ ] MP-011 Add PTY crash recovery tests (spawn/child death handling).
- [ ] MP-012 Add integration tests for IPC session event loop behavior.
- [ ] MP-013 Add concurrency stress tests for multi-threaded audio capture paths.
- [ ] MP-014 Run mutation testing locally and record score.
- [ ] MP-015 Improve mutation score by adding targeted tests for high-value paths.
- [ ] MP-016 Stress test under heavy I/O to confirm bounded memory behavior.
- [ ] MP-017 Manual QA checklist: auto-voice status visibility, queue flush in insert/auto modes, prompt log off by default, two terminals running independently.
- [ ] MP-018 Verify CSI-u filtering fix in real sessions.
- [ ] MP-019 Verify transcript queueing while Codex is busy (waits for next prompt).

### P1 - Safety and code clarity
- [ ] MP-020 Add SAFETY comments for unsafe blocks in PTY and other unsafe sections.
- [ ] MP-021 Add module-level docs and public API docs for core modules (voice, audio, pty_session).
- [ ] MP-022 Add struct/field docs where purpose is non-obvious and document complex logic blocks.
- [ ] MP-023 Extract magic numbers to `config/defaults.rs` or named constants.
- [ ] MP-024 Remove or justify dead code (`manual_stop`).
- [ ] MP-025 Standardize error message format where inconsistent.
- [ ] MP-026 Add PTY child_exec error diagnostics (capture errno for failed syscalls).
- [ ] MP-027 Address stderr redirection race during model load (or document as acceptable).
- [ ] MP-028 Remove unnecessary clone in voice metrics message path.

### P2 - Observability, performance, and UX improvements
- [ ] MP-030 Add structured logging (tracing) for better diagnostics.
- [ ] MP-031 Add PTY health monitoring to detect hung processes.
- [ ] MP-032 Retry logic for transient audio device failures.
- [ ] MP-033 Add benchmarks to CI for latency regression detection.
- [ ] MP-034 Add mic-meter hotkey for calibration.
- [ ] MP-035 Optional HUD input preview while Codex is thinking.
- [ ] MP-036 Investigate processing delay/freeze after sending input while Codex is thinking (after queue fix).
- [ ] MP-037 Consider making PTY output channel capacity configurable.

## Archive log
- `docs/archive/2026-01-29-claudeaudit-completed.md`
- `docs/archive/2026-01-29-docs-governance.md`
