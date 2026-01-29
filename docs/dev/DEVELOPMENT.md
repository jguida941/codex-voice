# Development

## Contents

- [Project structure](#project-structure)
- [Building](#building)
- [Testing](#testing)
- [Contribution workflow](#contribution-workflow)
- [Code style](#code-style)
- [Testing philosophy](#testing-philosophy)

## Project structure

```
codex-voice/
├── .github/
│   ├── CONTRIBUTING.md   # Contribution guidelines
│   ├── SECURITY.md       # Security policy
│   └── workflows/        # CI workflows
├── Codex Voice.app/      # macOS double-click launcher
├── agents.md             # SDLC policy and release checklist
├── QUICK_START.md        # Fast setup and commands
├── docs/
│   ├── CHANGELOG.md        # Release history
│   ├── CLI_FLAGS.md        # Full CLI and env reference
│   ├── INSTALL.md          # Install options and PATH notes
│   ├── TROUBLESHOOTING.md  # Common issues and fixes
│   ├── USAGE.md            # Controls and overlay behavior
│   ├── active/             # Active plans and work in progress
│   │   ├── MASTER_PLAN.md        # Active work and verification
│   │   ├── claude_review.md      # Required release review (move to archive after ship)
│   │   ├── BACKLOG.md            # Deprecated pointer (see master plan)
│   │   ├── MODULARIZATION_PLAN.md # Refactor checklist
│   │   ├── visual.md             # Visual design audit and plan
│   ├── archive/            # Completed work entries
│   └── dev/                # Reference and architecture docs
│       ├── ARCHITECTURE.md       # Architecture diagrams and data flow
│       ├── DEVELOPMENT.md        # Build/test workflow
│       ├── SDLC.md               # Deprecated pointer to agents.md
│       └── adr/                  # Architecture decision records
├── img/                 # Screenshots
├── rust_tui/            # Rust overlay + voice pipeline
│   └── src/
│       ├── bin/codex_overlay/main.rs # Overlay entry point
│       ├── app/         # TUI state + logging
│       ├── audio/       # CPAL recording, VAD, resample
│       ├── codex/       # Provider backend + PTY worker
│       ├── config/      # CLI flags + validation
│       ├── ipc/         # JSON IPC mode
│       ├── pty_session/ # PTY wrapper
│       ├── voice.rs     # Voice capture orchestration
│       ├── mic_meter.rs # Ambient/speech level sampler
│       ├── stt.rs       # Whisper transcription
│       └── ui.rs        # Full TUI rendering
├── scripts/             # Setup and test scripts
├── models/              # Whisper GGML models
├── start.sh             # Linux/macOS launcher
└── install.sh           # One-time installer
```

## Building

```bash
# Rust overlay
cd rust_tui && cargo build --release --bin codex-voice

# Rust backend (optional dev binary)
cd rust_tui && cargo build --release
```

## Testing

```bash
# Rust tests
cd rust_tui && cargo test

# Overlay tests
cd rust_tui && cargo test --bin codex-voice

# Perf smoke (voice metrics)
cd rust_tui && cargo test --no-default-features app::tests::perf_smoke_emits_voice_metrics -- --nocapture

# Memory guard (thread cleanup)
cd rust_tui && cargo test --no-default-features app::tests::memory_guard_backend_threads_drop -- --nocapture

# Mutation tests (CI enforces 80% minimum score)
cd rust_tui && cargo mutants --timeout 300 -o mutants.out
python3 ../scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80
```

## Contribution workflow

- Open or comment on an issue for non-trivial changes so scope and UX expectations are aligned.
- Keep UX tables/controls lists and docs in sync with behavior.
- Update `docs/CHANGELOG.md` for user-facing changes and note verification steps in PRs.

## Code style

- Rust: run `cargo fmt` and `cargo clippy --workspace --all-features -- -D warnings`.
- Keep changes small and reviewable; avoid unrelated refactors.
- Prefer explicit error handling in user-facing flows (status line + logs) so failures are observable.

## Testing philosophy

- Favor fast unit tests for parsing, queueing, and prompt detection logic.
- Add regression tests when fixing a reported bug.
- Run at least `cargo test` locally for most changes; add targeted bin tests for overlay-only work.
