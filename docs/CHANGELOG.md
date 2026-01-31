# Changelog

All notable changes to this project will be documented here, following the SDLC policy defined in `agents.md`.
Note: Some historical entries reference internal documents that are not published in this repository.

## [1.0.28] - 2026-01-31

### UX + Visuals
- Theme picker overlay (Ctrl+Y) with numbered selection.
- Live waveform + dB meter during recording in the status line.
- Transcript preview snippet shown briefly after transcription.
- Help/status shortcuts refreshed; overlay panels follow the active theme.
- Compact HUD modules now surface queue depth and last capture latency when available.

### Audio Feedback
- Optional notification sounds: `--sounds`, `--sound-on-complete`, `--sound-on-error`.

### CLI
- New `--backend` flag for selecting Codex/Claude/Gemini/Aider/OpenCode or a custom command (defaults to Codex).
- Backend-specific prompt patterns are used when available; Codex continues to auto-learn prompts by default.
- `--backend` custom commands now accept quoted arguments.

### Docs
- Updated usage, quick start, CLI flags, README, install, troubleshooting, and architecture/development docs to match the new options.

## [1.0.27] - 2026-01-31

### UX + Visuals
- Launcher now lists `?` help and theme flags in the startup tables.
- Launcher now documents available themes and `--no-color`.

### Docs
- Architecture doc now includes the overlay visual system components.
- Added modularization audit plan doc for historical tracking.

## [1.0.26] - 2026-01-31

### UX + Visuals
- **Overlay status line**: structured layout with mode/pipeline/sensitivity, themed colors, and automatic ANSI fallback.
- **Help overlay**: press `?` to show the shortcut panel (any key closes it).
- **Startup banner**: display version + config summary on launch.
- **Mic meter**: `--mic-meter` now renders a visual bar display alongside the suggested threshold.
- **Session summary**: print transcript/session stats on exit when activity is present.

### CLI
- **New flags**: `--theme` (coral/catppuccin/dracula/nord/ansi/none) and `--no-color`.
- **NO_COLOR support**: standard env var disables colors in the overlay.

### Fixes
- Status line refreshes when state changes even if the message text stays the same.
- Truncated status messages keep their original indicator/color for consistent meaning.
- Help overlay rendering clamps to terminal height to avoid scrolling in small terminals.

## [1.0.25] - 2026-01-29

### Docs
- Refresh README messaging, requirements, and controls summary for clearer onboarding.
- Reorganize README navigation and contributing links for a tighter user/developer split.
- Document the release review location in `docs/dev/DEVELOPMENT.md` and update master plan source inputs.

## [1.0.24] - 2026-01-29

### Build + Release
- **Version bump**: update `rust_tui/Cargo.toml` to 1.0.24 and align `Codex Voice.app` Info.plist.

### Refactor
- **Rust modularization**: split large modules (`ipc`, `pty_session`, `codex`, `audio`, `config`, `app`, and overlay helpers) into focused submodules with tests preserved.
- **Test access**: keep test-only hooks and visibility intact to avoid mutation/test regressions.

### Docs
- **Doc layout sync**: update architecture/development/visual docs to match the new module layout.
- **Active plan**: mark the modularization plan complete and document the post-split layout.
- **Policy links**: align SDLC/changelog references and backlog paths for consistent navigation.

## [1.0.23] - 2026-01-29

### Docs
- **README layout**: move macOS app (folder picker) section below UI modes.
- **macOS app version**: align `Codex Voice.app` Info.plist to 1.0.23.
- **Auto-voice status copy**: clarify that "Auto-voice enabled" means auto-voice is on while idle.
- **Usage guidance**: tighten wording for mode selection and long-dictation tips.
- **Usage layout**: add a mode matrix table that shows how listening and send modes combine.
- **Usage modes**: consolidate voice mode details into a single chart and move long-dictation notes into the same section.
- **Usage notes**: add prompt-detection fallback and Python fallback behavior notes.
- **Usage polish**: add a contents list, fix Quick Start wording, and include a `--lang` example.
- **Troubleshooting layout**: reorganize sections to reduce repetition and improve scanability.
- **Docs structure**: move dev docs under `docs/dev` and active plans under `docs/active` (update links).
- **Troubleshooting links**: make Quick Fixes entries clickable jump links.
- **Docs navigation**: add contents lists to README and Install docs.
- **CLI flags accuracy**: correct prompt log defaults and voice silence tail default.
- **ADR tracking**: keep ADRs under `docs/dev/adr` and track them in git.
- **Backlog cleanup**: remove duplicate queue item and normalize headings.
- **Dev docs navigation**: add contents lists to architecture, development, and modularization docs.
- **Docs formatting**: replace em dashes with hyphen separators for consistency.
- **SDLC policy**: move policy to `docs/dev/SDLC.md` and point the changelog at a tracked file.
- **Repo hygiene**: add `LICENSE`, `CONTRIBUTING.md`, and `SECURITY.md`.
- **README badges**: add CI, perf, memory guard, mutation testing, and license badges.
- **Docs navigation**: add "See Also" tables to install, CLI flags, and troubleshooting docs.
- **Dev docs**: expand contribution workflow, code style, and testing philosophy guidance.
- **Legacy CLI docs**: clarify deprecated status and mark quick start as non-functional.

## [1.0.22] - 2026-01-29

### Docs
- **macOS app visibility**: restore the folder-picker app path in README/Quick Start/Install docs.
- **macOS app version**: align `Codex Voice.app` Info.plist to 1.0.22.

## [1.0.21] - 2026-01-29

### Build + Release
- **Whisper crates compatibility**: align `whisper-rs` to the latest compatible 0.14.x release to avoid `links = "whisper"` conflicts.
- **Status redraw refactor**: reduce argument fanout in the overlay status redraw helper (clippy clean).
- **macOS app version**: align `Codex Voice.app` Info.plist to 1.0.21.

## [1.0.20] - 2026-01-29

### UX + Controls
- **Auto-voice startup**: auto mode now begins listening immediately when enabled (no silent wait).
- **Auto-voice status**: keep "Auto-voice enabled" visible on startup and when toggling on.
- **Status line stability**: defer status redraws until output is quiet to prevent ANSI garbage in the prompt.
- **Insert-mode rearm**: auto-voice re-arms immediately after transcripts when using insert send mode.
- **Capture limit**: max configurable capture duration raised to 60s (default still 30s).
- **Sensitivity hotkey alias**: `Ctrl+/` now also decreases mic sensitivity (same as `Ctrl+\`).
- **Transcript queueing**: once a prompt is detected, transcripts now wait for the next prompt instead of auto-sending on idle.
- **Prompt detection**: default prompt detection now auto-learns the prompt line (no default regex).

### Reliability + Privacy
- **Logging opt-in**: debug logs are disabled by default; enable with `--logs` (add `--log-content` for prompt/transcript snippets).
- **Prompt log opt-in**: prompt detection logs are disabled by default unless `--prompt-log` is set.
- **Log caps**: debug and prompt logs now rotate to avoid unbounded growth.
- **Buffer caps**: overlay input/writer channels, PTY combined output, and TUI input buffers are bounded.
- **Queue safety**: transcript queue drops now warn in the status line.
- **Security hardening**: `--claude-cmd` is sanitized; `--claude-skip-permissions` is configurable.

### Whisper + Audio
- **VAD smoothing**: new `--voice-vad-smoothing-frames` reduces flapping in noisy rooms.
- **Silence tail default**: reduced to 1000ms for lower latency.
- **Whisper tuning**: added `--lang auto`, `--whisper-beam-size`, and `--whisper-temperature`.
- **Capture metrics**: dropped audio frames are surfaced in the status line when present.

### Tests
- **New coverage** for mic meter calculations, STT error paths, UI input handling, transcript queue drop/flush, and config validation.

### Docs
- **README refresh**: streamlined quick start and moved deep sections into focused docs.
- **New guides**: added install, usage, CLI flags, troubleshooting, and development docs.
- **CLI flags**: consolidated into a single doc with codex-voice and rust_tui sections, plus missing flags and log env vars.

## [1.0.19] - 2026-01-29

### Changes
- **Transcript flush**: queued transcripts now auto-send after a short idle period (not just on prompt).
- **Queue merge**: queued transcripts are merged into a single message when flushed.
- **New flag**: `--transcript-idle-ms` controls the idle threshold for transcript auto-send.
- **CSI-u handling**: input parser now properly drops CSI-u sequences (avoids garbage text in the prompt).

## [1.0.17] - 2026-01-29

### Fixes
- **Auto-voice status spam**: avoid repeated status updates on empty captures.
- **Transcript queue**: only advances prompt gating when a newline is sent (fixes stuck queues in insert mode).
- **Prompt detection**: default regex `^>\\s?` to match Codex prompt reliably.
- **Status dedupe**: avoid re-sending identical status lines.

## [1.0.16] - 2026-01-29

### Changes
- **Binary rename**: `codex-voice` is now the only user-facing command (no `codex-overlay`).
- **Prompt log path**: configured via `--prompt-log` or `CODEX_VOICE_PROMPT_LOG` (no default unless set).
- **Env cleanup**: `CODEX_OVERLAY_PROMPT_*` is no longer supported; use `CODEX_VOICE_PROMPT_*`.
- **Docs/scripts**: update build/run instructions to use `codex-voice`.

## [1.0.15] - 2026-01-29

### Fixes
- **Overlay build fix**: remove stray duplicate block that broke compilation in `codex-voice` (source: `codex_overlay.rs`).

## [1.0.14] - 2026-01-29

### UX + Controls
- **Sensitivity hotkeys**: `Ctrl+]` / `Ctrl+\` adjust mic sensitivity (no Ctrl++/Ctrl+-).
- **Mic meter mode**: add `--mic-meter` plus ambient/speech duration flags to recommend a VAD threshold.
- **Startup/README updates**: refresh shortcut and command hints to match the new bindings.
- **Transcript queue**: when Codex is busy, transcripts are queued and sent on the next prompt; status shows queued count.

## [1.0.13] - 2026-01-28

### Build Fixes
- **Clippy clean**: resolve lint warnings across audio, codex, IPC, and PTY helpers for a clean CI run.

## [1.0.12] - 2026-01-28

### Testing & Reliability
- **Mutation coverage expansion**: add test hooks and integration tests across PTY, IPC, Codex backend, and overlay paths.
- **Overlay input/ANSI handling**: refactor input parsing and ANSI stripping for more robust control-sequence handling.
- **Audio pipeline hardening**: refactor recorder module and tighten resample/trimming behavior for stability.

## [1.0.11] - 2026-01-28

### Testing & Quality
- **Mutation coverage improvements**: expand PTY session tests and internal counters to harden mutation kills.
- **Mutation CI threshold**: mutation-testing workflow now enforces an 80% minimum score.

## [1.0.10] - 2026-01-25

### Build Fixes
- **Mutation testing baseline**: create a stub pipeline script during tests when the repo root is not present.

## [1.0.9] - 2026-01-25

### Build Fixes
- **Clippy cleanup in codex-voice**: resolve collapsible-if, map_or, clamp, and question-mark lints under `-D warnings` (source: `codex_overlay.rs`).

## [1.0.8] - 2026-01-25

### Build Fixes
- **SIGWINCH handler type**: cast the handler to `libc::sighandler_t` to satisfy libc 0.2.180 on Unix.
- **CI formatting cleanup**: apply `cargo fmt` so the rust-tui workflow passes.

## [1.0.7] - 2026-01-25

### Build Fixes
- **AtomicBool import for VAD stop flag**: fixes CI builds when high-quality-audio is disabled.

## [1.0.6] - 2026-01-25

### Auto-Voice Behavior
- **Silence no longer stops auto-voice**: empty captures immediately re-arm instead of waiting for new PTY output.
- **Less UI noise on silence**: auto mode keeps a simple "Auto-voice enabled" status instead of spamming "No speech detected".

## [1.0.5] - 2026-01-25

### Voice Capture UX Fixes
- **Insert-mode Enter stops early**: pressing Enter while recording now stops capture and transcribes the partial audio.
- **Processing status stays visible** until transcription completes or an error/empty result arrives.
- **Auto-voice cancel is real**: disabling auto-voice (Ctrl+V) now stops the active capture instead of dropping the handle.
- **Python fallback cancel**: Enter in insert mode cancels the python fallback capture (no partial stop available).
- **LF/CRLF Enter support**: terminals sending LF or CRLF now trigger the Enter interception reliably.

### Error Handling
- **Manual stop with no samples** returns an empty transcript instead of a fallback error.

## [1.0.4] - 2026-01-25

### Fast Local Transcription Feature
- **Benchmarked STT latency**: ~250ms processing after speech ends (tested with real microphone input).
- **Added feature to README**: "Fast local transcription - ~250ms processing after speech ends, no cloud API calls".
- **Verified code path**: latency_measurement binary uses identical code path as codex-voice (same voice::start_voice_job → stt::Transcriber).

### Bug Fixes
- **Filter [BLANK_AUDIO]**: Whisper's `[BLANK_AUDIO]` token is now filtered from transcripts, preventing spam in auto-voice mode when user stops talking.
- **Mermaid diagram**: Converted ASCII "How It Works" diagram to proper Mermaid flowchart for GitHub rendering.

## [1.0.3] - 2026-01-25

### UI Styling Refresh
- **Modern TUI styling**: rounded borders, vibrant red theme, bold titles in Rust overlay.
- **Startup tables refresh**: Unicode box-drawing characters, matching red theme.
- **Updated banner**: accurate description - "Rust overlay wrapping Codex CLI / Speak to Codex with Whisper STT".
- **README screenshot**: added startup screenshot to img/startup.png.

### Startup UX Polish (2026-01-24) - COMPLETE
- **Codex Voice banner**: `start.sh` now uses the Rust launch banner from the legacy CLI.
- **Compact quickstart tables**: launch output shows quick controls + common commands in green tables.
- **Adaptive layout**: smaller banner + dual-color columns keep tables visible in shorter terminals.
- **Startup output test**: `scripts/tests/startup_output_test.sh` guards line widths.

### Simplified Install Flow (2026-01-23) - COMPLETE
- **New installer**: added `install.sh` plus `scripts/setup.sh install` to download the Whisper model, build the Rust overlay, and install a `codex-voice` wrapper.
- **Overlay-first defaults**: `scripts/setup.sh` now defaults to `install` so it builds the Rust overlay by default.
- **Docs updated**: README + QUICK_START now point to `./install.sh` and `codex-voice` for the simplest path.

### Rust-Only Docs + Launchers (2026-01-23) - COMPLETE
- **Docs sweep**: removed legacy CLI references from user-facing docs and the audit.
- **Launchers aligned**: `start.sh` and `scripts/setup.sh` now run overlay-only; Windows launcher points to WSL/macos/linux.
- **Backlog added**: `docs/active/BACKLOG.md` tracks follow-up work and open UX items.

### Overlay UX (2026-01-23) - COMPLETE
- **New hotkeys**: Ctrl+T toggles send mode (auto vs insert), Ctrl++/Ctrl+- adjust mic sensitivity in 5 dB steps.
- **Startup hints**: `start.sh` prints the key controls and common flag examples for non-programmers.

### Homebrew Runtime Fixes (2026-01-23) - COMPLETE
- **Prebuilt overlay reuse**: `start.sh` now uses `codex-voice` from PATH when available, skipping builds in Homebrew installs.
- **User-writable model storage**: model downloads fall back to `~/.local/share/codex-voice/models` when the repo/libexec is not writable.
- **Homebrew detection**: Homebrew installs always use the user model directory instead of libexec, even if libexec is writable.
- **Install wrapper safety**: skip existing global `codex-voice` commands and prefer safe locations unless overridden.

### Rust Overlay Mode + Packaging (2026-01-22) - COMPLETE
- **Added Rust overlay mode**: new `codex-voice` binary runs Codex in a PTY, forwards raw ANSI output, and injects voice transcripts as keystrokes.
- **Prompt-aware auto-voice**: prompt detection with idle fallback plus configurable regex overrides for auto-voice triggering.
- **Serialized output writer**: PTY output + status line rendering go through a single writer thread to avoid terminal corruption.
- **PTY passthrough improvements**: new raw PTY session that answers DSR/DA queries without stripping ANSI.
- **Resizing support**: SIGWINCH handling updates PTY size and keeps the overlay stable.
- **Startup/launcher updates**: `start.sh` now defaults to overlay, ensures a Whisper model exists, and passes `--whisper-model-path`; macOS app launcher now uses overlay mode.
- **Docs refresh**: new `ARCHITECTURE.md` with detailed Rust-only diagrams and flows; README expanded with install paths, commands, and Homebrew instructions.
- **Repo hygiene**: internal architecture/archive/reference directories are now ignored by git and removed from the tracked set.

### Project Cleanup + macOS Launcher (2026-01-11) - COMPLETE
- **Added macOS app launcher**: `Codex Voice.app` now in repo alongside `start.sh` and `start.bat` for cross-platform consistency.
- **Major project structure cleanup**:
  - Removed duplicate files from `rust_tui/` (CHANGELOG, docs/, screenshots, etc.)
  - Moved rust_tui test scripts to `rust_tui/scripts/`
  - Consolidated scripts: deleted redundant launchers (`run_tui.sh`, `launch_tui.py`, `run_in_pty.py`)
  - Moved benchmark scripts to `scripts/tests/`
  - Deleted legacy folders (`stubs/`, `tst/`)
  - Kept `codex_voice.py` as legacy Python fallback
- **Updated all README diagrams** to match actual project structure.
- **Updated .gitignore** to exclude internal dev docs (`PROJECT_OVERVIEW.md`, `agents.md`, etc.)
- **Fixed Cargo.toml** reference to deleted test file.
- **82 Rust tests passing**.

### PTY Readiness + Auth Flow (2026-01-11) - COMPLETE
- **PTY readiness handshake**: wait for initial output and fail fast when only control output appears, preventing 20-30s stalls on persistent sessions.
- **/auth login flow**: new IPC command + wrapper command runs provider login via /dev/tty, with auth_start/auth_end events and raw mode suspension in TS.
- **Output delivery fix**: Codex Finished output is delivered even if the worker signal channel disconnects.
- **CI/testing updates**: added `mutation-testing.yml` and extended integration test coverage for the auth command.

### Provider-Agnostic Backend + JSON IPC (2026-01-10) - COMPLETE
- **Implemented provider-agnostic backend**: `rust_tui/src/ipc.rs` rewritten with non-blocking event loop, supporting both Codex and Claude CLIs with full slash-command parity.
- **IPC client flow functional**: JSON IPC supports voice capture, provider switching, and full event streaming.
- **Rust IPC mode**: `--json-ipc` flag enables JSON-lines protocol with capability handshake on startup.
- **All critical bugs fixed**:
  - IPC no longer blocks during job processing (stdin reader thread)
  - Codex/Claude output streams to IPC clients
  - Ctrl+R wired for voice capture (raw mode)
  - Unknown `/` commands forwarded to provider
- **New features**:
  - Capability handshake with full system info (`capabilities` event)
  - Session-level provider switching (`/provider claude`)
  - One-off provider commands (`/codex <prompt>`, `/claude <prompt>`)
  - Setup script for Whisper model download (`scripts/setup.sh`)
- **Test coverage**:
  - 18 unit tests for provider routing and IPC protocol
  - 12 integration tests for end-to-end flow
  - All tests passing

### CRITICAL - Phase 2B Design Correction (2025-11-13 Evening)
- **Rejected original Phase 2B "chunked Whisper" proposal (Option A)** after identifying fatal architectural flaw: sequential chunk transcription provides NO latency improvement (capture + Σchunks often slower than capture + single_batch). Original proposal would have wasted weeks implementing slower approach.
- **Documented corrected design** in the internal architecture archive (2025-11-13), specifying streaming mel + Whisper FFI architecture (Option B) as only viable path to <750ms voice latency target. User confirmed requirement: "we need to do option 2 i want it to be responsive not slow".
- **Design includes:** Three-worker parallel architecture (capture/mel/stt), streaming Whisper FFI wrapper, fallback ladder (streaming→batch→Python), comprehensive measurement gates, 5-6 week implementation plan, and mandatory approval gates before any coding begins.
- **Measurement gate executed:** Recorded 10 end-to-end Ctrl+R→Codex runs (short/medium commands) and captured results in the internal architecture archive (2025-11-13). Voice pipeline averages 1.19 s, Codex averages 9.2 s (≈88 % of total latency), so Phase 2B remains blocked until Codex latency is addressed or stakeholders explicitly accept the limited ROI.
- **Codex remediation plan:** Authored the Codex latency plan in the internal architecture archive (2025-11-13), covering telemetry upgrades, PTY/CLI health checks, CLI profiling, and alternative backend options. Phase 2B remains gated on executing this plan and obtaining stakeholder approval.
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
- Documented the approved Phase 1 backend plan (“Option 2.5” event-stream refactor) in the internal architecture archive (2025-11-13), capturing the Wrapper Scope Correction + Instruction blocks required before touching Codex integration.
- Added perf/memory guard rails: `app::tests::{perf_smoke_emits_timing_log,memory_guard_backend_threads_drop}` together with `.github/workflows/perf_smoke.yml` and `.github/workflows/memory_guard.yml` so CI enforces telemetry output and backend thread cleanup.
- Implemented the Phase 1 backend refactor: `CodexBackend`/`BackendJob` abstractions with bounded queues, `CliBackend` PTY ownership, App wiring to backend events, and new queue/tests (`cargo test --no-default-features`).
- Benchmark harness for Phase 2A: `audio::offline_capture_from_pcm`, the `voice_benchmark` binary, and `scripts/benchmark_voice.sh` capture deterministic short/medium/long clip metrics that feed the capture_ms SLA (internal benchmark notes live in the 2025-11-13 archive).

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
- Established baseline governance artifacts: `master_index.md`, repository `CHANGELOG.md`, root `PROJECT_OVERVIEW.md` (planned updates), and the initial dated architecture folder in the internal archive.
- Consolidated legacy documentation into the daily architecture tree (internal archive), the references set, and audits; backfilled the original architecture overview into the internal archive (2025-11-11).
- Relocated root-level guides (`ARCHITECTURE.md`, `MASTER_DOC.md`, `plan.md`) into the internal references set, corrected the historical architecture baseline to the internal archive (2025-11-11), and updated navigation pointers accordingly.
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
