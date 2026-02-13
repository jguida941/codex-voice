# Development

## Contents

- [Project structure](#project-structure)
- [Building](#building)
- [Testing](#testing)
- [Manual QA checklist](#manual-qa-checklist)
- [Contribution workflow](#contribution-workflow)
- [Pre-refactor docs readiness checklist](#pre-refactor-docs-readiness-checklist)
- [Code style](#code-style)
- [Testing philosophy](#testing-philosophy)

## Project structure

```
voxterm/
├── .github/
│   ├── CONTRIBUTING.md   # Contribution guidelines
│   ├── SECURITY.md       # Security policy
│   └── workflows/        # CI workflows
├── app/
│   ├── macos/VoxTerm.app # macOS double-click launcher
│   └── windows/          # Windows launcher (planned placeholder)
├── AGENTS.md             # SDLC policy and release checklist
├── QUICK_START.md        # Fast setup and commands
├── guides/
│   ├── CLI_FLAGS.md        # Full CLI and env reference
│   ├── INSTALL.md          # Install options and PATH notes
│   ├── TROUBLESHOOTING.md  # Common issues and fixes
│   ├── USAGE.md            # Controls and overlay behavior
│   └── WHISPER.md          # Whisper model guidance
├── dev/
│   ├── ARCHITECTURE.md     # Architecture diagrams and data flow
│   ├── CHANGELOG.md        # Release history
│   ├── DEVELOPMENT.md      # Build/test workflow
│   ├── active/             # Active plans and work in progress
│   ├── archive/            # Completed work entries
│   ├── adr/                # Architecture decision records
│   └── scripts/            # Developer scripts
│       ├── release.sh         # GitHub release script
│       ├── update-homebrew.sh # Homebrew tap update
│       ├── check_mutation_score.py # Mutation score helper
│       └── tests/             # Test scripts
├── img/                 # Screenshots
├── Makefile             # Developer tasks
├── src/                 # Rust overlay + voice pipeline
│   └── src/
│       ├── bin/voxterm/main.rs # Overlay entry point
│       ├── bin/voxterm/banner.rs # Startup splash + banner config
│       ├── bin/voxterm/help.rs # Shortcut overlay rendering
│       ├── bin/voxterm/terminal.rs # Terminal sizing + signal handling
│       ├── bin/voxterm/audio_meter/ # Mic meter UI (`--mic-meter`)
│       ├── bin/voxterm/hud/ # HUD modules and right panel visuals
│       ├── bin/voxterm/status_line/ # Status line layout + formatting
│       ├── bin/voxterm/settings/ # Settings overlay
│       ├── bin/voxterm/transcript/ # Transcript queue + delivery
│       ├── bin/voxterm/voice_control/ # Voice capture lifecycle
│       ├── bin/voxterm/input/ # Input parsing + events
│       ├── bin/voxterm/writer/ # Output writer + overlays
│       ├── bin/voxterm/theme/ # Theme definitions
│       ├── legacy_tui/   # Codex TUI state + logging (legacy)
│       ├── audio/       # CPAL recording, VAD, resample
│       ├── backend/     # AI CLI backend presets (overlay)
│       ├── codex/       # Codex-specific backend + PTY worker (TUI/IPC)
│       ├── config/      # CLI flags + validation
│       ├── ipc/         # JSON IPC mode
│       ├── pty_session/ # PTY wrapper
│       ├── voice.rs     # Voice capture orchestration
│       ├── mic_meter.rs # Ambient/speech level sampler
│       ├── stt.rs       # Whisper transcription
│       ├── auth.rs      # Backend auth helpers
│       ├── doctor.rs    # Diagnostics report
│       ├── telemetry.rs # Structured trace logging
│       ├── terminal_restore.rs # Panic-safe terminal restore
│       └── legacy_ui.rs  # Codex TUI rendering (legacy)
├── scripts/
│   ├── README.md        # Script documentation
│   ├── install.sh       # One-time installer
│   ├── start.sh         # Linux/macOS launcher
│   ├── setup.sh         # Model download and setup
│   └── python_fallback.py # Python fallback pipeline
├── whisper_models/      # Whisper GGML models
└── bin/                 # Install wrapper (created by install.sh)
```

Note: `src/` is the Rust workspace root and the crate lives under `src/src/`. This layout is intentional (workspace + crate separation).

## Building

```bash
# Rust overlay
cd src && cargo build --release --bin voxterm

# Rust backend (optional dev binary)
cd src && cargo build --release
```

## Testing

```bash
# Rust tests
cd src && cargo test

# Overlay tests
cd src && cargo test --bin voxterm

# Perf smoke (voice metrics)
cd src && cargo test --no-default-features legacy_tui::tests::perf_smoke_emits_voice_metrics -- --nocapture

# Memory guard (thread cleanup)
cd src && cargo test --no-default-features legacy_tui::tests::memory_guard_backend_threads_drop -- --nocapture

# Mutation tests (CI enforces 80% minimum score)
cd src && cargo mutants --timeout 300 -o mutants.out
python3 ../dev/scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80

# Mutation tests (offline/sandboxed; use a writable cache)
rsync -a ~/.cargo/ /tmp/cargo-home/
cd src && CARGO_HOME=/tmp/cargo-home CARGO_TARGET_DIR=/tmp/cargo-target CARGO_NET_OFFLINE=true cargo mutants --timeout 300 -o mutants.out
python3 ../dev/scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80

# Mutation helper script (module filter + offline env)
python3 ../dev/scripts/mutants.py --module overlay --offline --cargo-home /tmp/cargo-home --cargo-target-dir /tmp/cargo-target

# Summarize top paths with survived mutants
python3 ../dev/scripts/mutants.py --results-only --top 10

# Plot hotspots (top 25% by default)
python3 ../dev/scripts/mutants.py --results-only --plot --plot-scope dir --plot-top-pct 25
```

`--results-only` auto-detects the most recent `outcomes.json` under `src/mutants.out/`.
Mutation runs can be long; plan to run them overnight and use Ctrl+C to stop if needed.

## Dev CLI (devctl)

Unified CLI for common dev workflows:

```bash
# Core checks (fmt, clippy, tests, build)
python3 dev/scripts/devctl.py check

# Match CI scope (fmt-check + clippy + tests)
python3 dev/scripts/devctl.py check --profile ci

# Pre-push scope (CI + perf + mem loop)
python3 dev/scripts/devctl.py check --profile prepush

# Quick scope (fmt-check + clippy only)
python3 dev/scripts/devctl.py check --profile quick

# Mutants wrapper (offline cache)
python3 dev/scripts/devctl.py mutants --module overlay --offline \
  --cargo-home /tmp/cargo-home --cargo-target-dir /tmp/cargo-target

# Check mutation score only
python3 dev/scripts/devctl.py mutation-score --threshold 0.80

# Docs check (user-facing changes must update docs + changelog)
python3 dev/scripts/devctl.py docs-check --user-facing

# Governance hygiene audit (archive + ADR + scripts docs)
python3 dev/scripts/devctl.py hygiene

# Generate a report (JSON/MD)
python3 dev/scripts/devctl.py report --format json --output /tmp/devctl-report.json

# Include recent GitHub Actions runs (requires gh auth)
python3 dev/scripts/devctl.py status --ci --format md

# Pipe report output to a CLI that accepts stdin (requires login)
python3 dev/scripts/devctl.py report --format md --pipe-command codex
python3 dev/scripts/devctl.py report --format md --pipe-command claude
# If your CLI needs a stdin flag, pass it via --pipe-args.
```

Implementation layout:
- `dev/scripts/devctl.py`: thin entrypoint wrapper
- `dev/scripts/devctl/cli.py`: argument parsing and dispatch
- `dev/scripts/devctl/commands/`: per-command implementations
- `dev/scripts/devctl/common.py`: shared helpers (run_cmd, env, output)
- `dev/scripts/devctl/collect.py`: git/CI/mutation summaries for reports

## Manual QA checklist

- [ ] Auto-voice status visibility: REC tag + meter while capture is active.
- [ ] Queue flush works in both insert and auto send modes.
- [ ] Prompt logging is off by default unless explicitly enabled.
- [ ] Two terminals can run independently without shared state leaks.

## Contribution workflow

- Open or comment on an issue for non-trivial changes so scope and UX expectations are aligned.
- Keep UX tables/controls lists and docs in sync with behavior.
- Update `dev/CHANGELOG.md` for user-facing changes and note verification steps in PRs.

## Pre-refactor docs readiness checklist

Use this checklist before larger UI/behavior refactors to avoid documentation drift:

- [ ] `README.md` updated (features list, screenshots, quick overview).
- [ ] `QUICK_START.md` updated (install steps and common commands).
- [ ] `guides/USAGE.md` updated (controls, status messages, theme list).
- [ ] `guides/CLI_FLAGS.md` updated (flags and defaults).
- [ ] `guides/INSTALL.md` updated (dependencies, setup steps, PATH notes).
- [ ] `guides/TROUBLESHOOTING.md` updated (new known issues/fixes).
- [ ] `img/` screenshots refreshed if UI output/controls changed.

## Code style

- Rust: run `cargo fmt` and `cargo clippy --workspace --all-features -- -D warnings`.
- Keep changes small and reviewable; avoid unrelated refactors.
- Prefer explicit error handling in user-facing flows (status line + logs) so failures are observable.

## Testing philosophy

- Favor fast unit tests for parsing, queueing, and prompt detection logic.
- Add regression tests when fixing a reported bug.
- Run at least `cargo test` locally for most changes; add targeted bin tests for overlay-only work.

## CI/CD Workflow

GitHub Actions run on every push and PR:

| Workflow | File | What it checks |
|----------|------|----------------|
| Rust TUI CI | `.github/workflows/rust_ci.yml` | Build, test, clippy, fmt |
| Voice Mode Guard | `.github/workflows/voice_mode_guard.yml` | Focused command/dictation/review mode regressions |
| Perf Smoke | `.github/workflows/perf_smoke.yml` | Perf smoke test + metrics verification |
| Latency Guard | `.github/workflows/latency_guard.yml` | Synthetic latency regression guardrails |
| Memory Guard | `.github/workflows/memory_guard.yml` | 20x memory guard loop |
| Mutation Testing | `.github/workflows/mutation-testing.yml` | 80% minimum mutation score (scheduled) |

**Before pushing, run locally (recommended):**

```bash
# Core CI (matches rust_ci.yml)
make ci

# Full push/PR suite (adds perf smoke + memory guard loop)
make prepush

# Governance/doc architecture hygiene
python3 dev/scripts/devctl.py hygiene
```

**Manual equivalents (if you prefer direct cargo commands):**

```bash
cd src

# Format code
cargo fmt

# Lint (must pass with no warnings)
cargo clippy --workspace --all-features -- -D warnings

# Run tests
cargo test --workspace --all-features

# Check mutation score (optional, CI enforces this)
cargo mutants --timeout 300 -o mutants.out
python3 ../dev/scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80
```

**Check CI status:** [GitHub Actions](https://github.com/jguida941/voxterm/actions)

## Releasing

### Version bump

1. Update version in `src/Cargo.toml`
2. Update `dev/CHANGELOG.md` with release notes
3. Commit: `git commit -m "Release vX.Y.Z"`

### Create GitHub release

```bash
# Use the release script (recommended)
./dev/scripts/release.sh X.Y.Z

# Create release on GitHub
gh release create vX.Y.Z --title "vX.Y.Z" --notes "See CHANGELOG.md"
```

### Update Homebrew tap

```bash
# Use the update script (recommended)
./dev/scripts/update-homebrew.sh X.Y.Z
```

This automatically fetches the SHA256 and updates the formula.

Users can then upgrade:
```bash
brew update && brew upgrade voxterm
```

See `scripts/README.md` for full script documentation.

## Local development tips

**Test with different backends:**
```bash
voxterm              # Codex (default)
voxterm --claude     # Claude Code
voxterm --gemini     # Gemini CLI (experimental; not fully validated)
```

**Debug logging:**
```bash
voxterm --logs                    # Enable debug log
tail -f $TMPDIR/voxterm_tui.log   # Watch log output
tail -f $TMPDIR/voxterm_trace.jsonl  # Watch structured trace output (JSON)
```
