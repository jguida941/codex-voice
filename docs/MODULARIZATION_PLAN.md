# Rust Modularization Plan (Draft)

Goal: break up the largest Rust source files into cohesive modules without changing behavior,
keeping tests and mutation coverage intact. This is a structural refactor only.

## Scope (top offenders by LOC)

- `rust_tui/src/pty_session.rs` (~2.6k)
- `rust_tui/src/ipc.rs` (~2.6k)
- `rust_tui/src/bin/codex_overlay.rs` (~2.5k)
- `rust_tui/src/codex.rs` (~1.9k)
- `rust_tui/src/audio.rs` (~1.7k)
- `rust_tui/src/config.rs` (~1.3k)
- `rust_tui/src/app.rs` (~1.1k)

## Principles

- No behavior changes.
- Keep public API surface stable (`pub`/`pub(crate)` re-exports from the original module).
- Move tests with their code to avoid accidental coverage loss.
- Preserve mutation-test hooks and counters.
- Prefer mechanical moves + minimal renames in the first pass.

## Proposed Module Layout

### 1) codex_overlay (largest, highest churn)
Create `rust_tui/src/bin/codex_overlay/` with:

- `mod.rs` (current main loop; re-export public items)
- `config.rs` (OverlayConfig, VoiceSendMode)
- `input.rs` (InputEvent, input parser, input thread)
- `writer.rs` (WriterMessage, writer thread, status redraw)
- `prompt.rs` (PromptTracker, prompt regex logic, prompt logger)
- `voice.rs` (VoiceManager + voice helpers)
- `transcript.rs` (pending queue, merge/flush logic)
- `tests.rs` (tests currently at file end)

### 2) ipc
Create `rust_tui/src/ipc/`:

- `mod.rs` (public API: run_ipc_mode, types)
- `protocol.rs` (IPC request/response types, serde)
- `router.rs` (command dispatch, provider selection)
- `session.rs` (provider session loops, stdio event loop)
- `tests.rs`

### 3) pty_session
Create `rust_tui/src/pty_session/`:

- `mod.rs` (PtyOverlaySession public API)
- `pty.rs` (pty open/fork, fd handling)
- `io.rs` (read/write helpers, output buffering)
- `osc.rs` (OSC/DSR/DA handling)
- `counters.rs` (test/mutants counters + overrides)
- `tests.rs`

### 4) codex backend
Create `rust_tui/src/codex/`:

- `mod.rs` (public API, re-exports)
- `backend.rs` (BackendJob, BackendEvent, stats)
- `pty_backend.rs` (CliBackend/PTY logic)
- `cli.rs` (process invocation and output capture)
- `tests.rs`

### 5) audio
Create `rust_tui/src/audio/`:

- `mod.rs` (public API, re-exports)
- `vad.rs` (VadConfig, engine, smoothing)
- `capture.rs` (CaptureState/FrameAccumulator/metrics)
- `resample.rs` (resampler paths)
- `dispatch.rs` (FrameDispatcher)
- `tests.rs`

### 6) config + app
Smaller splits to improve readability:

- `rust_tui/src/config/`:
  - `mod.rs` (AppConfig, VoicePipelineConfig)
  - `defaults.rs` (constants + defaults)
  - `validation.rs` (validate())
  - `tests.rs`

- `rust_tui/src/app/`:
  - `mod.rs` (App public API)
  - `logging.rs` (init_logging, log_debug)
  - `state.rs` (App fields + helpers)
  - `tests.rs`

## Phased Execution Plan

1) **codex_overlay** split (highest UX impact, fastest payoff)
   - Create directory + move structs/functions.
   - Update `mod.rs` to re-export as needed.
   - Keep `main()` in `mod.rs`.

2) **pty_session** and **ipc** split
   - Keep test hooks with their modules.
   - Run `cargo test --bin codex-voice` after each split.

3) **codex** backend split
   - Keep event structs and stats in `backend.rs`.

4) **audio** split
   - Move VAD + capture pipeline into separate modules.
   - Ensure tests follow their code.

5) **config/app** split
   - Keep CLI surface and validation intact.

## Test/CI Expectations

- `cargo fmt` and `cargo clippy --bin codex-voice` at each phase.
- `cargo test` (or at least `cargo test --bin codex-voice` + core lib tests).
- Mutation testing still executed in CI (no local changes needed).

## Risks + Mitigations

- **Mutation coverage regression**: keep test hooks with their modules; avoid logic changes.
- **Visibility mistakes**: re-export in `mod.rs` and use `pub(crate)` to avoid breakage.
- **Circular deps**: keep module boundaries strict; use small helper modules to break cycles.

