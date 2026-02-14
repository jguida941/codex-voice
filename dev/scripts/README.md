# Developer Scripts

Scripts for development, testing, and releases.

**Tip:** Use the Makefile for common tasks: `make help`

Common pre-push checks:
```bash
make ci       # Core CI (fmt + clippy + tests)
make prepush  # All push/PR checks (ci + perf smoke + memory guard)
```

## Scripts

| Script | Purpose | Usage |
|--------|---------|-------|
| `release.sh` | Create GitHub release tag | `./dev/scripts/release.sh 1.0.33` |
| `update-homebrew.sh` | Update Homebrew formula | `./dev/scripts/update-homebrew.sh 1.0.33` |
| `publish-pypi.sh` | Build/publish PyPI package | `./dev/scripts/publish-pypi.sh --upload` |
| `devctl.py` | Unified dev CLI (checks, mutants, release, report) | `python3 dev/scripts/devctl.py` |
| `mutants.py` | Interactive mutation testing | `python3 dev/scripts/mutants.py` |
| `check_mutation_score.py` | Verify mutation score | Used by CI |

## release.sh

Creates a Git tag for a new release.

```bash
./dev/scripts/release.sh 1.0.33
```

**Prerequisites:**
- On `master` branch
- No uncommitted changes
- `src/Cargo.toml` version matches
- `dev/CHANGELOG.md` has entry

## update-homebrew.sh

Updates the Homebrew tap formula with new version and SHA256.

```bash
./dev/scripts/update-homebrew.sh 1.0.33
```

## publish-pypi.sh

Builds the `pypi/` package and optionally uploads to PyPI.

```bash
# Build + validate artifacts only
./dev/scripts/publish-pypi.sh

# Build + upload (requires PyPI credentials configured for twine)
./dev/scripts/publish-pypi.sh --upload
```

## mutants.py

Interactive mutation testing with module selection.

```bash
# Interactive mode
python3 dev/scripts/mutants.py

# Test all modules
python3 dev/scripts/mutants.py --all

# Test specific module
python3 dev/scripts/mutants.py --module audio

# Run a single shard (matches CI sharding)
python3 dev/scripts/mutants.py --module overlay --shard 1/8

# List available modules
python3 dev/scripts/mutants.py --list

# Show last results
python3 dev/scripts/mutants.py --results-only

# Offline/sandboxed run with writable cache/target dirs
rsync -a ~/.cargo/ /tmp/cargo-home/
python3 dev/scripts/mutants.py --module overlay --offline --cargo-home /tmp/cargo-home --cargo-target-dir /tmp/cargo-target

# Show top paths for survived mutants
python3 dev/scripts/mutants.py --results-only --top 10

# Plot hotspots (top 25% by default)
python3 dev/scripts/mutants.py --results-only --plot --plot-scope dir --plot-top-pct 25
```

`--results-only` auto-detects the most recent `outcomes.json` under `src/mutants.out/`.
Mutation runs can be long; plan to run them overnight and use Ctrl+C to stop if needed.
The scheduled GitHub mutation workflow shards runs (`1/8` ... `8/8`) and enforces one aggregated 80% threshold.

Or use the Makefile:
```bash
make mutants         # Interactive
make mutants-audio   # Audio module only
make mutants-results # Show results
```

## devctl.py

Unified dev CLI for common workflows (checks, mutants, mutation score, docs-check, release, status/report).

Notes:
- Source lives under `dev/scripts/devctl/` with modular command implementations.
- Uses existing scripts under `dev/scripts/` and does not duplicate logic.
- `--format json|md` outputs machine-readable reports (good for pasting into tools).
- `--pipe-command` can forward reports to another CLI that accepts stdin.
- Release/homebrew subcommands require confirmation and refuse to run in CI by default.
- Profiles for `check`: `ci`, `prepush`, `release`, `quick`.
- `hygiene` audits archive naming, ADR status/index consistency, and scripts-doc coverage.

Structure (high level):
- `dev/scripts/devctl.py`: thin entrypoint wrapper
- `dev/scripts/devctl/cli.py`: argument parsing and dispatch
- `dev/scripts/devctl/commands/`: per-command implementations
- `dev/scripts/devctl/common.py`: shared helpers (run_cmd, env, output)
- `dev/scripts/devctl/collect.py`: git/CI/mutation summaries for reports

```bash
# Core checks (fmt, clippy, tests, build)
python3 dev/scripts/devctl.py check

# Match CI scope (fmt-check + clippy + tests)
python3 dev/scripts/devctl.py check --profile ci

# Pre-push scope (CI + perf + mem loop)
python3 dev/scripts/devctl.py check --profile prepush

# Quick scope (fmt-check + clippy only)
python3 dev/scripts/devctl.py check --profile quick

# Run mutants with offline cache
python3 dev/scripts/devctl.py mutants --module overlay --offline \
  --cargo-home /tmp/cargo-home --cargo-target-dir /tmp/cargo-target

# Run one shard through devctl
python3 dev/scripts/devctl.py mutants --module overlay --shard 1/8

# Check mutation score only
python3 dev/scripts/devctl.py mutation-score --threshold 0.80

# Docs check (user-facing changes must update docs + changelog)
python3 dev/scripts/devctl.py docs-check --user-facing

# Governance hygiene audit (archive + ADR + scripts docs)
python3 dev/scripts/devctl.py hygiene

# Plot top 25% hotspots by directory
python3 dev/scripts/devctl.py mutants --results-only --plot --plot-scope dir --plot-top-pct 25

# Produce a report (JSON/MD)
python3 dev/scripts/devctl.py report --format json --output /tmp/devctl-report.json

# Include recent GitHub Actions runs (requires gh auth)
python3 dev/scripts/devctl.py status --ci --format md

# Pipe report output into a CLI (requires stdin support and login)
python3 dev/scripts/devctl.py report --format md \
  --pipe-command codex
python3 dev/scripts/devctl.py report --format md \
  --pipe-command claude
# If your CLI needs a stdin flag, pass it via --pipe-args.
```

Makefile shortcuts:
```bash
make dev-check
make dev-ci
make dev-prepush
make dev-mutants
make dev-mutation-score
make dev-docs-check
make dev-hygiene
make dev-list
make dev-status
make dev-report
```

## tests/

Test scripts for benchmarking and integration testing.

| Script | Purpose |
|--------|---------|
| `benchmark_voice.sh` | Voice pipeline performance |
| `measure_latency.sh` | End-to-end latency profiling + synthetic CI guardrails |
| `integration_test.sh` | IPC protocol testing |

Latency commands:
```bash
# Baseline synthetic latency (no mic) with the measurement harness
./dev/scripts/tests/measure_latency.sh --synthetic --voice-only --skip-stt --count 5

# CI-friendly regression guardrails (fails on threshold violations)
./dev/scripts/tests/measure_latency.sh --ci-guard --count 3
```

---

## Release Workflow

```bash
# 1. Update version in src/Cargo.toml
# 2. Update dev/CHANGELOG.md
# 3. Commit all changes
git add -A && git commit -m "Release v1.0.33"

# 4. Create tag and push
./dev/scripts/release.sh 1.0.33

# 5. Create GitHub release
gh release create v1.0.33 --title "v1.0.33" --notes "See CHANGELOG.md"

# 6. Publish PyPI package
./dev/scripts/publish-pypi.sh --upload

# 7. Update Homebrew
./dev/scripts/update-homebrew.sh 1.0.33
```

Or use: `make release V=1.0.33 && make homebrew V=1.0.33`
