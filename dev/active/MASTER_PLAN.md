# Master Plan (Active, Unified)

## Canonical Plan Rule
- This file is the single active plan for strategy, execution, and release tracking.
- `dev/active/overlay.md` is reference research only (market/competitor + UX audit), not an execution plan.
- Deferred work lives in `dev/deferred/` and must be explicitly reactivated here before implementation.

## Status Snapshot (2026-02-14)
- Last tagged release: `v1.0.63` (2026-02-14)
- Current release target: `v1.0.64`
- Active development branch: `develop`
- Release branch: `master`
- Strategic focus: visual HUD differentiation (telemetry richness, transitions, visualization modes)

## Strategic Direction
- Protect current moat: terminal-native PTY orchestration, prompt-aware queueing, local-first voice flow.
- Close trust gap: latency metrics must match user perception and be auditable.
- Build differentiated product value in phases:
  1. Visual-first UX pass (telemetry, motion, layout polish, theme ergonomics)
  2. Workflow differentiators (voice navigation, history, CLI workflow polish)
  3. Advanced expansion (streaming STT, tmux/neovim, accessibility)

## Phase 0 - Completed Release Stabilization (v1.0.51-v1.0.52)
- [x] MP-072 Prevent HUD/timer freeze under continuous PTY output.
- [x] MP-073 Improve IDE terminal input compatibility and hidden-HUD discoverability.
- [x] MP-074 Update docs for HUD/input behavior changes and debug guidance.
- [x] MP-075 Finalize latency display semantics to avoid misleading values.
- [x] MP-076 Add latency audit logging and regression tests for displayed latency behavior.
- [x] MP-077 Run release verification (`cargo build --release --bin voiceterm`, tests, docs-check).
- [x] MP-078 Finalize release notes, bump version, tag, push, GitHub release, and Homebrew tap update.
- [x] MP-096 Expand SDLC agent governance: post-push audit loop, testing matrix by change type, CI expansion policy, and per-push docs sync requirements.
- [x] MP-099 Consolidate overlay research into a single reference source (`dev/active/overlay.md`) and mirror candidate execution items in this plan.

## Phase 1 - Latency Truth and Observability
- [x] MP-079 Define and document latency terms (capture, STT, post-capture processing, displayed HUD latency).
- [x] MP-080 Hide latency badge when reliable latency cannot be measured.
- [x] MP-081 Emit structured `latency_audit|...` logs for analysis.
- [x] MP-082 Add automated tests around latency calculation behavior.
- [x] MP-097 Fix busy-output HUD responsiveness and stale meter/timer artifacts (settings lag under Codex output, stale REC duration/dB after capture, clamp meter floor to stable display bounds).
- [x] MP-098 Eliminate blocking PTY input writes in the overlay event loop so queued/thinking backend output does not stall live typing responsiveness.
- [x] MP-083 Run and document baseline latency measurements with `latency_measurement` and `dev/scripts/tests/measure_latency.sh` (`dev/archive/2026-02-13-latency-baseline.md`).
- [x] MP-084 Add CI-friendly synthetic latency regression guardrails (`.github/workflows/latency_guard.yml` + `measure_latency.sh --ci-guard`).
- [x] MP-111 Add governance hygiene automation for archive/ADR/script-doc drift (`python3 dev/scripts/devctl.py hygiene`) and codify archive/ADR lifecycle policy.
- [x] MP-122 Prevent mutation-lane timeout by sharding scheduled `cargo mutants` runs and enforcing one aggregated score across shards.

## Phase 2 - Overlay Quick Wins
- [x] MP-085 Voice macros and custom triggers (`.voiceterm/macros.yaml`).
- [x] MP-086 Runtime macros ON/OFF toggle (settings state + transcript transform gate).
- [x] MP-087 Restore baseline send-mode semantics (`auto`/`insert`) without an extra review-first gate.
- [x] MP-112 Add CI voice-mode regression lane for macros-toggle and send-mode behavior (`.github/workflows/voice_mode_guard.yml`).
- [ ] MP-088 Persistent user config (`~/.config/voiceterm/config.toml`) for core preferences (deferred until visual sprint is complete).

## Phase 2A - Visual HUD Sprint (Current Priority)
- [x] MP-101 Add richer HUD telemetry visuals (sparkline/chart/gauge) with bounded data retention.
- [x] MP-100 Add animation transition framework for overlays and state changes (TachyonFX or equivalent).
- [x] MP-054 Optional right-panel visualization modes in minimal HUD.
- [x] MP-105 Add adaptive/contextual HUD layouts and state-driven module expansion.
- [x] MP-113 Tighten startup splash ergonomics and IDE compatibility (shorter splash duration, reliable teardown in IDE terminals, corrected startup tagline centering, and better truecolor detection for JetBrains-style terminals).
- [x] MP-114 Polish startup/theme UX in IDE terminals (remove startup top-gap, keep requested themes on 256-color terminals, and render Theme Picker neutral when current theme is `none`).
- [x] MP-115 Stabilize terminal compatibility regressions (drop startup arrow escape noise during boot, suppress Ctrl+V idle pulse dot beside `PTT`, and restore conservative ANSI fallback when truecolor is not detected).
- [x] MP-116 Fix JetBrains terminal HUD duplication by hardening scroll-region cursor restore semantics.
- [x] MP-117 Prevent one-column HUD wrap in JetBrains terminals (status-banner width guard + row truncation safety).
- [x] MP-118 Harden cross-terminal HUD rendering and PTY teardown paths (universal one-column HUD safety margin, writer-side row clipping to terminal width, and benign PTY-exit write error suppression on shutdown).
- [x] MP-119 Restore the stable `v1.0.53` writer/render baseline for Full HUD while retaining the one-column layout safety margin to recover reliable IDE terminal rendering.
- [x] MP-120 Revert unstable post-release scroll-region protection changes that reintroduced severe Full HUD duplication/corruption during active Codex output.
- [x] MP-121 Harden JetBrains startup/render handoff by auto-skipping splash in IDE terminals and clearing stale HUD/overlay rows on resize before redraw.
- [x] MP-123 Harden PTY/IPC backend teardown to signal process groups and reap child processes, with regression tests that verify descendant cleanup.
- [x] MP-124 Add Full-HUD border-style customization (including borderless mode) and keep right-panel telemetry explicitly user-toggleable to `Off`.
- [x] MP-125 Fix HUD right-panel `Anim only` behavior so idle state keeps a static panel visible instead of hiding the panel until recording.
- [x] MP-126 Complete product/distribution naming rebrand to VoiceTerm across code/docs/scripts/app launcher, and add a PyPI launcher package scaffold (`pypi/`) for `voiceterm`.
- [ ] MP-104 Add explicit voice-state visualization (idle/listening/processing/responding) with clear transitions.
- [ ] MP-055 Quick theme switcher in settings.
- [ ] MP-102 Add toast notification center with auto-dismiss, severity, and history review.

## Phase 3 - Overlay Differentiators
- [ ] MP-090 Voice terminal navigation actions (scroll/copy/error/explain).
- [ ] MP-091 Searchable transcript history and replay workflow.

## Phase 4 - Advanced Expansion
- [ ] MP-092 Streaming STT and partial transcript overlay.
- [ ] MP-093 tmux/neovim integration track.
- [ ] MP-094 Accessibility suite (fatigue hints, quiet mode, screen-reader compatibility).
- [ ] MP-095 Custom vocabulary learning and correction persistence.

## Backlog (Not Scheduled)
- [ ] MP-015 Improve mutation score with targeted high-value tests.
- [ ] MP-016 Stress test heavy I/O for bounded-memory behavior.
- [ ] MP-031 Add PTY health monitoring for hung process detection.
- [ ] MP-032 Add retry logic for transient audio device failures.
- [ ] MP-033 Add benchmarks to CI for latency regression detection.
- [ ] MP-034 Add mic-meter hotkey for calibration.
- [ ] MP-037 Consider configurable PTY output channel capacity.
- [ ] MP-103 Add searchable command palette for settings/actions/macros with keyboard-first flow.
- [ ] MP-106 Add hybrid voice+keyboard transcript input panel for correction and selective send.
- [ ] MP-107 Add session dashboard with voice metrics (latency/WPM/error rate) and export path.
- [ ] MP-108 Prototype contextual autocomplete/suggestion dropdowns for macros/corrections.
- [ ] MP-109 Evaluate block-based voice command history UI against PTY/session constraints.

## Deferred Plans
- `dev/deferred/DEV_MODE_PLAN.md` (paused until Phases 1-2 outcomes are complete).
- MP-089 LLM-assisted voice-to-command generation (optional local/API provider) is deferred; current product focus is Codex/Claude CLI-native flow quality, not an additional LLM mediation layer.

## Release Policy (Checklist)
1. `src/Cargo.toml` version bump
2. `dev/CHANGELOG.md` entry finalized
3. Verification pass for change scope
4. Tag + push from `master`
5. GitHub release creation
6. PyPI package publish
7. Homebrew tap formula update + push

## Execution Gate (Every Feature)
1. Create or link an MP item before implementation.
2. Implement the feature and add/update tests in the same change.
3. Run SDLC verification for scope:
   `python3 dev/scripts/devctl.py check --profile ci`
4. Run docs coverage check for user-facing work:
   `python3 dev/scripts/devctl.py docs-check --user-facing`
5. Update required docs (`dev/CHANGELOG.md` + relevant guides) before merge.
6. Push only after checks pass and plan/docs are aligned.

## References
- Execution + release tracking: `dev/active/MASTER_PLAN.md`
- Market, competitor, and UX evidence: `dev/active/overlay.md`
- SDLC policy: `AGENTS.md`
- Architecture: `dev/ARCHITECTURE.md`
- Changelog: `dev/CHANGELOG.md`

## Archive Log
- `dev/archive/2026-01-29-claudeaudit-completed.md`
- `dev/archive/2026-01-29-docs-governance.md`
- `dev/archive/2026-02-01-terminal-restore-guard.md`
- `dev/archive/2026-02-01-transcript-queue-flush.md`
- `dev/archive/2026-02-02-release-audit-completed.md`
