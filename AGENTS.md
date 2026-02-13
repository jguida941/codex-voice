# Agents

This file is the canonical SDLC and release policy for this repo.

## Product vision
VoxTerm is a polished, voice-first overlay for AI CLIs. Primary support for
**Codex** and **Claude Code**. Gemini CLI support is in progress. It begins as
a listening/transcription helper but is designed to evolve into a full HUD:
discoverable controls, settings, history, and feedback that enhance the CLI
terminal experience without replacing it.

## Quick navigation
- `CLAUDE.md` (local AI entrypoint; gitignored; points back to this file)
- `dev/active/MASTER_PLAN.md` (single canonical strategy + execution plan)
- `dev/active/overlay.md` (market research reference, not execution plan)
- `dev/deferred/` (paused plans not in active execution)
- `dev/archive/2026-02-02-release-audit-completed.md` (completed code audit)
- `dev/archive/` (completed work entries)
- `dev/archive/README.md` (archive retention and naming policy)
- `dev/ARCHITECTURE.md` (system architecture)
- `dev/DEVELOPMENT.md` (build/test workflow)
- `dev/adr/` (architecture decisions)
- `dev/adr/README.md` (ADR index + status lifecycle policy)
- `dev/CHANGELOG.md` (release history)
- `dev/scripts/README.md` (dev tooling and devctl usage)
- `README.md` and `QUICK_START.md` (user-facing docs)

## Before you start
- Read `dev/active/MASTER_PLAN.md` for current strategy, phase, and release scope.
- Use `dev/active/overlay.md` only for market/competitor reference context.
- Check git status and avoid unrelated changes.
- Confirm scope and whether a release/version bump is needed.

## Branching model (required)
- Long-lived branches: `master` (release/tag branch) and `develop` (active integration branch).
- Start all non-release work from `develop` using short-lived `feature/<topic>` or `fix/<topic>` branches.
- Merge feature/fix branches back into `develop` only after checks pass.
- Promote release candidates from `develop` to `master`, then tag and publish from `master`.
- Delete merged feature/fix branches locally and on origin to keep branch history clean.
- Do not introduce additional long-lived branches unless the change is tracked in `dev/active/MASTER_PLAN.md`.

## SDLC policy (user-facing changes)
- Update docs for user-facing behavior changes (see Documentation Checklist below).
- Add or update an entry in `dev/CHANGELOG.md` with a clear summary and the correct date.
- For releases, bump `src/Cargo.toml` version and align docs with the new version.
- Run verification before shipping. Minimum is a local build of `voxterm`.
- Keep UX tables/controls lists in sync with actual behavior.
- If UI output or flags change, update any screenshots or tables that mention them.

## Feature delivery workflow (required)
Apply this sequence for every feature/fix:
1. Add or link a task in `dev/active/MASTER_PLAN.md`.
2. Implement code changes.
3. Add or update tests in the same change.
4. Run verification (`python3 dev/scripts/devctl.py check --profile ci` minimum).
5. Update docs and run `python3 dev/scripts/devctl.py docs-check --user-facing` for user-facing changes.
   - If architecture/workflow/lifecycle/CI/release mechanics changed, update `dev/ARCHITECTURE.md` in the same change.
6. Commit and push only after checks pass.

## One-command feedback loop (recommended)
Use this exact loop so the agent can self-audit continuously:
```bash
python3 dev/scripts/devctl.py check --profile ci
python3 dev/scripts/devctl.py docs-check --user-facing
python3 dev/scripts/devctl.py hygiene
python3 dev/scripts/devctl.py status --ci --format md
```

For release work:
1. Bump `src/Cargo.toml` and finalize `dev/CHANGELOG.md`.
2. Tag/push release from `master` (or via `dev/scripts/release.sh`).
3. Publish GitHub release (`gh release create ...`).
4. Update Homebrew tap using `./dev/scripts/update-homebrew.sh <version>`.

## Post-push audit loop (required)
After each push, run this loop before ending the session:
1. Verify branch/tag state is correct (`git status`, `git log`, tags as needed).
2. Verify CI status (`python3 dev/scripts/devctl.py status --ci --format md` or Actions UI).
3. If CI fails, add/adjust a `MASTER_PLAN` item and rerun checks until green.
4. Re-validate docs alignment for any behavior/flag/UI changes.
5. Run governance hygiene audit (`python3 dev/scripts/devctl.py hygiene`) and fix any hard failures.

## Testing matrix by change type (required)
- Overlay/input/status/HUD changes:
  - `python3 dev/scripts/devctl.py check --profile ci`
  - `cd src && cargo test --bin voxterm`
- Performance/latency-sensitive changes:
  - `python3 dev/scripts/devctl.py check --profile prepush`
  - `./dev/scripts/tests/measure_latency.sh --voice-only --synthetic` (baseline runs)
  - `./dev/scripts/tests/measure_latency.sh --ci-guard` (synthetic regression guardrails)
- Threading/lifecycle/memory changes:
  - `cd src && cargo test --no-default-features legacy_tui::tests::memory_guard_backend_threads_drop -- --nocapture`
- Mutation-hardening work:
  - `python3 dev/scripts/devctl.py mutation-score --threshold 0.80`
  - optional: `python3 dev/scripts/devctl.py mutants --module overlay`
- Release candidate validation:
  - `python3 dev/scripts/devctl.py check --profile release`

## Code review (mandatory after implementation)

After implementing any code change, review for:

- **Security**: injection, XSS, unsafe input handling, hardcoded secrets
- **Memory**: unbounded buffers, leaks, missing caps, large allocations
- **Errors**: unwrap/expect in non-test code, missing error paths, silent failures
- **Concurrency**: deadlocks, race conditions, lock contention in callbacks
- **Performance**: unnecessary allocations, blocking in async, hot loops
- **Style**: clippy warnings, formatting, naming conventions, dead code

Do not consider implementation complete until self-review passes.

## Verification (local)

Minimum:

```bash
cd src && cargo build --release --bin voxterm
```

Common checks (run what matches your change):

```bash
# Format + lint
cd src && cargo fmt --all -- --check
cd src && cargo clippy --workspace --all-features -- -D warnings

# Tests
cd src && cargo test
cd src && cargo test --bin voxterm
```

Targeted checks mirrored in CI:

```bash
# Perf smoke (voice metrics)
cd src && cargo test --no-default-features legacy_tui::tests::perf_smoke_emits_voice_metrics -- --nocapture

# Memory guard (thread cleanup)
cd src && cargo test --no-default-features legacy_tui::tests::memory_guard_backend_threads_drop -- --nocapture

# Mutation testing (heavy; usually on demand)
cd src && cargo mutants --timeout 300 -o mutants.out
python3 ../dev/scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80
```

## Dev CLI (recommended)

Use the unified dev CLI for common workflows:

```bash
# Core checks
python3 dev/scripts/devctl.py check

# CI scope (fmt-check + clippy + tests)
python3 dev/scripts/devctl.py check --profile ci

# Pre-push scope (CI + perf + mem loop)
python3 dev/scripts/devctl.py check --profile prepush

# User-facing docs enforcement
python3 dev/scripts/devctl.py docs-check --user-facing

# Mutation score only
python3 dev/scripts/devctl.py mutation-score --threshold 0.80
```

## CI workflows (reference)
- `rust_ci.yml`: format, clippy, and test for `src/`.
- `perf_smoke.yml`: perf smoke test and voice metrics verification.
- `latency_guard.yml`: synthetic latency regression guardrails.
- `memory_guard.yml`: repeated memory guard test.
- `mutation-testing.yml`: scheduled mutation testing with threshold check.

## CI expansion policy
Add or update CI when new risk classes are introduced:
- New latency-sensitive logic: add/extend perf or latency guard coverage.
- New long-running threads/background workers: add loop/soak memory guards.
- New release/distribution mechanics: add validation for release/homebrew scripts.
- New user modes/flags: ensure at least one integration lane exercises them.

## Documentation checklist

When making changes, check which docs need updates:

**Always check:**
| Doc | Update when |
|-----|-------------|
| `dev/CHANGELOG.md` | **Every user-facing change** (required) |
| `README.md` | Project structure, quick start, or major features change |
| `QUICK_START.md` | Install steps or basic usage changes |

**User-facing behavior:**
| Doc | Update when |
|-----|-------------|
| `guides/USAGE.md` | Controls, modes, status messages, or UX changes |
| `guides/CLI_FLAGS.md` | Any flag added, removed, or default changed |
| `guides/INSTALL.md` | Install methods, dependencies, or setup steps change |
| `guides/TROUBLESHOOTING.md` | New known issues or fixes discovered |

**Developer/architecture:**
| Doc | Update when |
|-----|-------------|
| `dev/ARCHITECTURE.md` | Module structure, data flow, design changes, **or any workflow/lifecycle/CI/release mechanics added/changed/removed** |
| `dev/DEVELOPMENT.md` | Build process, testing, or contribution workflow changes |
| `dev/adr/` | Significant design decisions (see ADRs below) |

**Do not skip docs.** Missing doc updates cause drift and user confusion.

## Documentation update flow (apply on behavior changes)
1. Scan `dev/active/MASTER_PLAN.md` for related items and update it if scope changes.
2. Update `dev/CHANGELOG.md` for user-facing changes with the correct date.
3. Review user-facing docs: `README.md`, `QUICK_START.md`, `guides/USAGE.md`, `guides/CLI_FLAGS.md`,
   `guides/INSTALL.md`, `guides/TROUBLESHOOTING.md`.
   - Use `python3 dev/scripts/devctl.py docs-check --user-facing` to validate doc coverage.
4. Update developer docs if needed: `dev/ARCHITECTURE.md`, `dev/DEVELOPMENT.md`.
   - `dev/ARCHITECTURE.md` is mandatory when architecture/workflow mechanics changed.
5. If UI output or flags change, update any screenshots/tables that mention them.
6. If a change introduces a new architectural decision, add an ADR in `dev/adr/` and update
   the ADR index.

## Documentation sync protocol (every push)
On every push, review docs and explicitly decide "updated" or "no change needed":
- Core: `dev/CHANGELOG.md`, `dev/active/MASTER_PLAN.md`
- User docs: `README.md`, `QUICK_START.md`, `guides/USAGE.md`, `guides/CLI_FLAGS.md`,
  `guides/INSTALL.md`, `guides/TROUBLESHOOTING.md`
- Dev docs: `dev/ARCHITECTURE.md`, `dev/DEVELOPMENT.md`, `dev/scripts/README.md`

Enforcement commands:
- `python3 dev/scripts/devctl.py docs-check --user-facing`
- Use strict mode for broad UX/flag changes:
  `python3 dev/scripts/devctl.py docs-check --user-facing --strict`

## ADRs (architecture decisions)
- Use `dev/adr/` for architecture-level decisions or cross-module changes.
- Include context, decision, and consequences.
- Use `NNNN-short-title.md` (zero-padded) and keep `dev/adr/README.md` index/status in sync.
- If a decision is replaced, create a new ADR and mark the older ADR `Status: Superseded`
  with `Superseded-by: ADR NNNN`.
- Do not rewrite historical ADR decisions; supersede them.

## Archive retention policy
- Keep `dev/archive/` entries as immutable historical records; do not delete completed audits/plans
  to reduce context size.
- Keep active execution in `dev/active/MASTER_PLAN.md`; archive files are reference/history only.
- If archive volume grows, add summary/index docs instead of deleting source records.

## Active work tracking
- Strategy, execution, and release tasks all live in `dev/active/MASTER_PLAN.md`.
- `dev/active/overlay.md` is reference-only market research.
- Deferred plans live in `dev/deferred/`.
- Move completed work to `dev/archive/` with a dated entry.

## Releases

**Using release scripts (recommended):**

```bash
# 1. Update version in src/Cargo.toml
# 2. Update dev/CHANGELOG.md with release notes
# 3. Commit all changes
# 4. Create GitHub tag
./dev/scripts/release.sh 1.0.33

# 5. Create GitHub release (after tag is pushed)
gh release create v1.0.33 --title "v1.0.33" --notes "See CHANGELOG.md"

# 6. Update Homebrew tap
./dev/scripts/update-homebrew.sh 1.0.33
```

**Manual steps (if scripts fail):**
- Bump `src/Cargo.toml` version and align docs with the new version.
- Tag: `git tag v1.0.33 && git push origin v1.0.33`
- Update the Homebrew tap formula (version + checksum) and push it.
- Verify a fresh install after updating the tap (`brew install` or `brew reinstall`).

## Homebrew tap

Tap repo: https://github.com/jguida941/homebrew-voxterm

**Automated update (recommended):**

```bash
./dev/scripts/update-homebrew.sh 1.0.33
```

This script:
- Fetches SHA256 from the GitHub release tarball
- Updates the formula with new version and checksum
- Commits and pushes to the tap repo

**Manual update process:**

1. **Get SHA256 of release tarball:**
   ```bash
   curl -sL https://github.com/jguida941/voxterm/archive/refs/tags/v1.0.33.tar.gz | shasum -a 256
   ```

2. **Update formula** in homebrew-voxterm repo:
   - Update `version` to new version
   - Update `url` to new tag
   - Update `sha256` with new checksum

3. **Push formula changes** to tap repo.

4. **Verify installation:**
   ```bash
   brew update
   brew reinstall voxterm
   voxterm --version
   ```

## Scope and non-goals
- Scope: Voice HUD overlay, Rust TUI, voice pipeline, and supporting docs/scripts for AI CLIs.
- Non-goals: hosted services, cloud telemetry, or altering upstream AI CLI behavior.

## End-of-session checklist
- [ ] Code changes have been self-reviewed (see Code Review section)
- [ ] Verification commands passed for changes made
- [ ] Documentation updated per Documentation Checklist
- [ ] `dev/CHANGELOG.md` updated if user-facing behavior changed
- [ ] `dev/active/MASTER_PLAN.md` updated and completed work moved to `dev/archive/`
- [ ] New issues discovered added to `dev/active/MASTER_PLAN.md`
- [ ] Follow-ups captured as new master plan items
- [ ] Git status is clean or changes are committed/stashed

## Key files reference

| Purpose | Location |
|---------|----------|
| Main binary | `src/src/bin/voxterm/main.rs` |
| App state | `src/src/legacy_tui/` |
| Audio pipeline | `src/src/audio/` |
| PTY handling | `src/src/pty_session/` |
| Codex backend | `src/src/codex/` |
| Config | `src/src/config/` |
| IPC | `src/src/ipc/` |
| STT/Whisper | `src/src/stt.rs` |
| Version | `src/Cargo.toml` |
| macOS app plist | `app/macos/VoxTerm.app/Contents/Info.plist` |

## Notes
- `dev/archive/2026-01-29-claudeaudit-completed.md` contains the production readiness checklist.
- If UI output or flags change, update any screenshots or tables that mention them.
- Always prefer editing existing files over creating new ones.
