# Changelog

All notable changes to this project will be documented here, following the SDLC policy defined in `AGENTS.md`.
Note: Some historical entries reference internal documents that are not published in this repository.

## [Unreleased]

### CI
- Add a dedicated latency guard workflow (`.github/workflows/latency_guard.yml`) that runs synthetic voice-only regression bounds in CI.
- Extend `latency_measurement` and `dev/scripts/tests/measure_latency.sh` with CI guardrail flags (`--ci-guard`, `--skip-stt`, and min/max latency bounds) for deterministic regression checks.

### Documentation
- Document latency baseline results in `dev/archive/2026-02-13-latency-baseline.md`.
- Document the latency guard workflow and command usage in `dev/scripts/README.md`.

### Developer Experience
- Add `python3 dev/scripts/devctl.py hygiene` to audit archive naming, ADR status/index consistency, and `dev/scripts` documentation coverage.
- Add archive retention and ADR supersession lifecycle policy updates across `AGENTS.md`, `dev/archive/README.md`, and `dev/adr/README.md`.

### UX
- Expand Claude prompt-ready matching to recognize confirmation prompts (for example `[Y/n]`) in addition to bare `>` prompt lines.
- Reserve the terminal scroll region above the HUD/overlay so long PTY output does not scroll through the bottom HUD rows.
- Clear stale recording duration/meter/preview visuals when auto-voice is disabled, including the capture-cancel path.

### Tests
- Add Claude backend prompt-pattern coverage for confirmation prompts.
- Add writer render coverage for setting and resetting terminal scroll-region sequences.

## [1.0.53] - 2026-02-13

### UX
- Improve HUD/overlay responsiveness while CLI output is continuously streaming by reducing writer redraw wait thresholds and forcing redraw opportunities during PTY output flow.
- Batch PTY output chunks in the event loop before forwarding to the writer to reduce message pressure and settings/HUD lag while Codex is busy.
- Use non-blocking PTY-output dispatch with deferred backpressure handling so busy output no longer blocks input/settings event handling.
- Queue and flush PTY input with non-blocking writes so typing and Enter forwarding remain responsive even when backend output is busy or PTY writes hit temporary backpressure.
- Clear stale capture visuals (recording duration + dB waveform/meter) when capture exits recording mode so the HUD does not freeze with old timer/meter values.
- Clamp live recording meter floor to `-60dB` for stable display semantics.

### Tests
- Add PTY write coverage for single-attempt non-blocking writes, including partial-write and `WouldBlock` behavior.

### Documentation
- Add troubleshooting guidance for delayed typing/Enter behavior under heavy backend output.

## [1.0.52] - 2026-02-13

### Documentation
- Promote `AGENTS.md` to a tracked repository policy file and remove the ignore rule that prevented policy updates from being versioned.
- Expand agent workflow requirements with a mandatory post-push audit loop, testing matrix by change type, CI expansion policy, and per-push documentation sync protocol.
- Unify active planning into a single canonical `dev/active/MASTER_PLAN.md` and clarify that `dev/active/overlay.md` is reference market research.

## [1.0.51] - 2026-02-13

### UX
- Prevent HUD freeze (REC duration, live dB meter, queue/heartbeat status) when PTY output is continuously active by running periodic overlay tasks independently of the `select!` timeout branch.
- Render the HUD/launcher immediately on startup so users always see VoxTerm controls without waiting for the first status update.
- Hidden HUD now shows a branded `VoxTerm` launcher row with a clickable `open` button and `Ctrl+U` hint while idle.
- Improve terminal compatibility for controls by parsing parameterized CSI arrow sequences and additional mouse protocols (URXVT + X10, alongside SGR).
- Allow Left/Right + Enter HUD button navigation even when mouse clicking is toggled off.
- Treat HUD latency as post-capture processing time and hide the badge when metrics are incomplete instead of showing a misleading estimate.

### Diagnostics
- Add `VOXTERM_DEBUG_INPUT=1` to log raw input byte sequences and decoded input events for IDE terminal compatibility debugging.
- Add `latency_audit|...` debug log lines with displayed/capture/STT/elapsed timing fields per transcript.

### Tests
- Validate the fix with local `voxterm` build/test checks to cover the queue + busy-output regression path.
- Add parser coverage for URXVT/X10 mouse events, partial X10 buffering, and parameterized CSI arrow sequences.
- Add hidden-HUD coverage for the idle launcher button/row behavior.
- Add unit coverage for latency calculation behavior (prefer STT timing, fallback from capture, hide when metrics are missing).

## [1.0.50] - 2026-02-09

### UX
- Buffer partial input escape sequences so SGR mouse scroll/motion does not leak into Claude output.

### Tests
- Ensure partial SGR mouse sequences are buffered across input reads.

## [1.0.49] - 2026-02-09

### UX
- Ignore non-click SGR mouse events so trackpad scroll/motion does not flood Claude output or stall the HUD timer.

### Tests
- Add an input parser check that wheel events are dropped.

## [1.0.48] - 2026-02-09

### UX
- Buffer partial PTY escape sequences so mouse reporting does not leak into Claude output.

### Tests
- Add coverage for buffering incomplete PTY escape sequences.

## [1.0.47] - 2026-02-09

### UX
- Preserve CRLF PTY output so line content is not erased during control-edit handling.

### Tests
- Cover CRLF handling for PTY control edits and PTY output capture in IPC tests.

## [1.0.46] - 2026-02-08

### Tests
- Stabilize PTY/IPC exit checks and avoid a racy token assertion.

### Chore
- Run rustfmt on test files and silence mutants-feature lint warnings.

## [1.0.45] - 2026-02-08

### Documentation
- Document `VOXTERM_NO_STARTUP_BANNER` in install and troubleshooting guides.
- Audit and refresh docs: clarify terminal injection vs backend submission, expand CLI flag coverage, and update troubleshooting/Whisper wording.
- Replace the Usage guide hero image with the logo and remove the stale hero asset.
- Address doc audit gaps: sensitivity labels, backend list, missing flags, archive links, model sizing notes, and Whisper search path notes.
- Add a version badge and Support section to the README.
- Add GitHub community files (code of conduct, issue templates, PR template).
- Add the HUD style shortcut (`Ctrl+U`) to Quick Start controls.

### CI
- Update perf/memory guard test selectors to `legacy_tui::tests` after the legacy rename.

### Tests
- Gate high-quality resampler test helpers behind feature flags to keep no-feature builds clean.
- Expand audio, IPC, and PTY coverage for cancellation, retry, and lifecycle edge cases.

## [1.0.44] - 2026-02-06

### UX
- Show the startup banner when launching via wrapper scripts; set `VOXTERM_NO_STARTUP_BANNER` to suppress it.

### Documentation
- Document `VOXTERM_NO_STARTUP_BANNER` in CLI environment variables.

## [1.0.43] - 2026-02-06

### UX
- Add `--login` to run backend CLI authentication (Codex/Claude) before launching the overlay.
- Honor backend-specific default themes when `--theme` is not provided.
- IPC mode now streams Claude output via PTY and reports transcript duration in `transcript` events.

### Diagnostics
- Add JSON structured tracing logs (default: `voxterm_trace.jsonl` in the temp directory) when logging is enabled.

### Documentation
- Reorder README sections for readability with a Quick Nav, early Requirements, and a grouped UI Tour.
- Add the README logo asset and restore the overlay-running screenshot in the Usage guide.
- Remove outdated theme picker screenshot notes.
- Document the new `--login` flag across Quick Start, Usage, CLI flags, Install, and Troubleshooting guides.

## [1.0.42] - 2026-02-03

### UX
- Render the startup splash before raw mode and normalize line endings so the logo lines don't drift.
- Show the startup splash before launching the CLI, keep it visible for 10s, then clear before Codex starts.

### Documentation
- Add a Codex backend screenshot to the README.
- Add a Hidden HUD screenshot to the Usage guide.
- Add installer output screenshot to the Install guide.

## [1.0.40] - 2026-02-03

### UX
- Center the ASCII art logo and add a tagline with version and keyboard shortcuts underneath.
- Show the ASCII banner with tagline instead of the separate config line for wide terminals.

## [1.0.39] - 2026-02-03

### UX
- Add shiny purple ASCII art logo banner on startup (displays when terminal is ≥66 columns wide).

### Documentation
- Reformat README Whisper model setup/requirements for readability and remove the roadmap section.
- Rewrite the install guide to recommend Homebrew and clarify the install paths.
- Move Homebrew update troubleshooting steps into the troubleshooting guide and link from install.
- Remove Gemini references from user-facing docs (README, Quick Start, Usage, CLI Flags, Install, Troubleshooting).
- Reflow user guides for readability (Quick Start, Usage, CLI Flags, Troubleshooting, Whisper).

### UX
- Show the startup logo as a temporary overlay after initial CLI output (visible for at least 10s) so it does not conflict with the Codex intro panel.

## [1.0.38] - 2026-02-03

### UX
- Theme picker now supports arrow-key navigation, Enter to select, and multi-digit theme numbers.

### Documentation
- Refresh the Claude backend screenshot.

## [1.0.37] - 2026-02-03

### UX
- Add a minimal HUD back button and make overlay controls clickable (theme rows + close buttons), with more reliable mouse/tap handling.
- Right panel brackets stay dim regardless of waveform color.
- Keep last meter waveform/dB and recording duration between recordings to reduce flicker, and remove the duplicated waveform from the main row.
- Remove the left-side sensitivity dB from the full HUD row to avoid redundant readings.
- Add a Heartbeat right-panel mode that shows a subtle pulse.
- Render "Ready" in the success color in full and minimal HUD modes.
- Remove the redundant Chips right-panel mode.
- Add HUD button navigation via Left/Right arrows (when Mouse is enabled) and clean up mouse click handling.
- Add Tokyo Night and Gruvbox themes to the picker.
- Mouse control is enabled by default (toggle it off in Settings).

### Bug Fixes
- Fix latency display to avoid bottoming out at 0ms when capture timing skews.

### Documentation
- Note that mouse control is enabled by default and update theme-picker click guidance.

**Remaining audit items (future work):**

### Code Quality
- Group writer thread state into a struct to simplify redraw logic (MP-049).

### Documentation
- Simplify backend docs to Codex + Claude, mark Gemini as in works, and remove references to other AI backends.
- Clarify Whisper model selection in install/quick start docs and call out the optional `--codex` flag.

## [1.0.36] - 2026-02-02

### UX
- Stabilize the full HUD main-row layout by reserving sensitivity/duration/meter slots with dim placeholders and widening the right-side panel with a VU label to prevent flicker.

### Documentation
- Refresh hero, theme picker, settings, and recording screenshots, and add a minimal HUD screenshot.

## [1.0.35] - 2026-02-02

### Features
- Add minimal mode HUD: `--hud-style minimal` or `--minimal-hud` shows a single-line strip (e.g., `◉ AUTO · Ready`, `● REC · -55dB`).
- Add hidden mode HUD: `--hud-style hidden` keeps a blank row when idle, only shows `REC` while recording.
- Add `Ctrl+U` hotkey to cycle HUD style (Full → Minimal → Hidden) at runtime.
- Add Claude and Codex themes; default theme now follows backend when `--theme` is not set.
- Add ChatGPT theme (`--theme chatgpt`) with emerald green brand color (#10a37f).

### Bug Fixes
- Stop duplicate status-line updates from spamming repeated "Transcript ready" messages by relying on the HUD banner.

### Code Quality (Code Audit - Phase 1 Quick Wins)
- Fix potential buffer bounds panic in CSI-u sequence parsing by validating minimum length before indexing.
- Add I/O error logging for stdout write/flush operations to improve debugging.
- Optimize waveform rendering by using iterator chains instead of Vec allocations.
- Add `#[inline]` hints to hot-path functions (display_width, level_color, rms_db, peak_db).
- Add `#[must_use]` attributes to key struct/function returns to catch accidental discards.
- Pre-allocate `meter_levels` Vec with METER_HISTORY_MAX capacity to reduce clone overhead.
- Consolidate METER_HISTORY_MAX constant to `status_line.rs` (single source of truth).
- Optimize hot-path formatters (format_shortcut_colored, format_mode_indicator, format_chip, format_pulse_dots) to use push_str instead of format! macros.
- Consolidate status-line formatting helpers to reduce duplication and improve maintainability.
- Reduce oversized handle_voice_message parameter list by introducing a context struct.
- Add pre-refactor docs readiness checklist to keep README/QUICK_START/USAGE/CLI_FLAGS/INSTALL/TROUBLESHOOTING and screenshots in sync.
- Add SAFETY comments around unsafe blocks in PTY, signal, and Whisper integration code.
- Add public API docs for core modules (voice, audio, pty_session) to clarify usage.
- Document prompt tracking and transcript queue structures plus batching logic for maintainability.
- Extract timing "magic numbers" into named constants in main loop, writer thread, and voice status handling.
- Gate the manual_stop helper behind test/mutant cfg to avoid dead code in release builds.
- Standardize PTY write error messages to use a consistent prefix.
- Emit errno details for child_exec failures before exiting the PTY child process.
- Document the brief, startup-only stderr redirect used during Whisper model load.
- Remove a redundant CaptureMetrics clone in the voice capture path.
- Add a manual QA checklist for auto-voice visibility, queue flush, prompt logging, and multi-terminal runs.
- Add docstrings/comments across devctl and dev scripts to improve maintainability.
- Add an integration test covering transcript delivery into the PTY session (voice → injection path).
- Add PTY reader thread EOF/disconnect tests to cover child-exit recovery behavior.
- Add IPC event-loop integration coverage for active job/voice/auth processing.
- Add concurrency stress test coverage for parallel voice jobs via the fallback pipeline.
- Harden arrow-key parsing against index overflow when scanning input bytes.

### Documentation
- Update README + usage theme counts to 11 and note that the theme picker screenshot needs refresh.
- Document offline mutation testing workflow and new mutants.py cache/target overrides.
- Extend mutants.py summaries with top files/directories and results paths for faster triage.
- Add mutants.py matplotlib hotspot plotting (top 25% by default) with a CLI flag.
- Add devctl.py unified dev CLI for checks, mutants, mutation score, releases, and reports.
- Modularize devctl into command modules under dev/scripts/devctl/ for easier extension.
- Allow devctl status/report outputs to be piped into other CLIs via --pipe-command.
- Add devctl profiles, list command, and docs-check to enforce user-facing guide updates.
- Note mutation runs can be long; recommend overnight runs and allow Ctrl+C to stop.

## [1.0.34] - 2026-02-02

### Bug Fixes
- Drain voice capture results even while PTY output is streaming so "Processing" clears and transcripts queue/send while the CLI is busy.
- In manual mode, queue transcripts while the CLI is busy and send after prompt/idle instead of silently injecting mid-stream.
- Fix dim text appearing blurry on Coral/ANSI themes by using dark gray instead of ANSI dim attribute.

### Documentation
- Document queued transcript behavior in the usage guide and troubleshooting status messages.

### UX
- Refresh the HUD control row with button-style shortcuts: dim brackets/keys, colored labels when active.
- Clearer button labels: `^V auto/ptt` (voice mode), `^T auto/insert` (send mode).
- Recording button shows red when active, yellow when processing.
- Auto-voice button shows blue when listening, dim when push-to-talk.
- Send mode button shows green for auto-send, yellow for insert mode.
- All button colors are theme-aware and match the current theme palette.
- Add a right-side HUD panel (Ribbon/Dots/Chips) with a recording-only animation toggle in Settings.

## [1.0.33] - 2026-02-02

### Reorganization
- Major codebase reorganization: `rust_tui/` → `src/`, `docs/` → `guides/` + `dev/`
- Rename Rust crate from `rust_tui` to `voxterm` to match project name
- Add Makefile for common developer commands
- Add `dev/scripts/mutants.py` for interactive mutation testing

### Bug Fixes
- Fix macOS app launcher breaking on paths containing apostrophes
- Fix Homebrew formula paths after repo reorganization
- Add explicit platform detection to setup and start scripts (macOS, Linux, Windows WSL2)
- Align macOS app version metadata with the 1.0.33 release

### UX
- Status banner top border now shows a VoxTerm label with theme-matched Vox/Term colors instead of the theme name

### Documentation
- Add `guides/WHISPER.md` for model selection guidance
- Add platform support table to installation docs
- Fix manual hotkeys in terminals that emit CSI-u key sequences (Ctrl+R/Ctrl+V/etc).
- Retry PTY writes on would-block errors so transcript injection is reliable under load.
- In manual mode, send transcripts immediately instead of waiting on prompt detection.
- Avoid auto-voice status flicker on empty captures; only surface dropped-frame notes.
- Skip duplicate startup banners when launched from wrapper scripts.
- Use ANSI save/restore for status redraws and improve themed banner background alignment.
- Reserve terminal rows for the status banner/overlays so CLI output no longer overlaps the HUD.
- Clear banner rows on every redraw to prevent stacked ghost lines after scrolling.
- Lower the default VAD threshold to -55 dB to improve voice detection on quieter mics.
- Suppress the auto-voice "Listening" status message so the meter display stays clean.
- Add a settings overlay with arrow-key navigation and button-style controls.
- Move pipeline labeling into the recording tag and shorten status labels to "Rust"/"Python".
- Use combined ANSI/DEC cursor save/restore to keep the input cursor stable across overlays.
- Fill the status banner background across the full row to avoid uneven tinting.
- Make Nord theme HUD backgrounds transparent to avoid a washed-out look on dark terminals.
- Automatically disable HUD background fills in Warp terminals to prevent black bars behind text.
- Restore cursor attributes after HUD draws to keep CLI colors intact.
- Show settings/help hints in the HUD idle message and overlay footers.
- Fix documentation links and dev test script paths after repo reorg (guides/dev).
- Align Homebrew tap instructions and CI workflow references with the new repo layout.

## [1.0.32] - 2026-02-02

### Bug Fixes
- Fix overlay border alignment in theme picker, settings, and help overlays.
- Fix Unicode character width calculations in overlay title lines.
- Remove background color applications from status line for transparent rendering.
- Simplify settings footer text to avoid Unicode width issues.

## [1.0.31] - 2026-02-02

### Bug Fixes
- Fix theme picker border alignment where right border extended too far.
- Fix status banner background color bleeding outside the box on themed overlays (Nord, etc.).
- Fix top border width calculation in status banner.

## [1.0.30] - 2026-02-02

### Branding (Breaking)
- Rename the project to VoxTerm across the CLI, docs, and UI strings.
- New primary command: `voxterm`.
- New env var prefix: `VOXTERM_*`.
- New config path: `~/.config/voxterm/`.
- New model path: `~/.local/share/voxterm/models`.
- New log files: `voxterm_tui.log` and `voxterm_crash.log`.
- macOS app renamed to `VoxTerm.app`.

### Privacy
- Avoid logging full panic details to the debug log unless `--log-content` is enabled.

### UX
- Refresh the startup banner styling and show backend/theme/auto state.

## [1.0.29] - 2026-02-02

### Reliability
- Add a terminal restore guard with a shared panic hook so raw mode/alternate screen clean up even on crashes.
- Emit a minimal crash log entry (metadata only unless log content is enabled).
- Add `--doctor` diagnostics output for terminal/config/audio visibility.
- Clear overlay panel regions when the height changes to avoid resize artifacts.
- Improve queued transcript flushing by allowing idle-based sends when prompt detection stalls after output finishes.

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
- Document the release review location in `dev/DEVELOPMENT.md` and update master plan source inputs.

## [1.0.24] - 2026-01-29

### Build + Release
- **Version bump**: update `rust_tui/Cargo.toml` to 1.0.24 and align `VoxTerm.app` Info.plist.

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
- **macOS app version**: align `VoxTerm.app` Info.plist to 1.0.23.
- **Auto-voice status copy**: clarify that "Auto-voice enabled" means auto-voice is on while idle.
- **Usage guidance**: tighten wording for mode selection and long-dictation tips.
- **Usage layout**: add a mode matrix table that shows how listening and send modes combine.
- **Usage modes**: consolidate voice mode details into a single chart and move long-dictation notes into the same section.
- **Usage notes**: add prompt-detection fallback and Python fallback behavior notes.
- **Usage polish**: add a contents list, fix Quick Start wording, and include a `--lang` example.
- **Troubleshooting layout**: reorganize sections to reduce repetition and improve scanability.
- **Docs structure**: move dev docs under `docs/dev` and active plans under `dev/active` (update links).
- **Troubleshooting links**: make Quick Fixes entries clickable jump links.
- **Docs navigation**: add contents lists to README and Install docs.
- **CLI flags accuracy**: correct prompt log defaults and voice silence tail default.
- **ADR tracking**: keep ADRs under `dev/adr` and track them in git.
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
- **macOS app version**: align `VoxTerm.app` Info.plist to 1.0.22.

## [1.0.21] - 2026-01-29

### Build + Release
- **Whisper crates compatibility**: align `whisper-rs` to the latest compatible 0.14.x release to avoid `links = "whisper"` conflicts.
- **Status redraw refactor**: reduce argument fanout in the overlay status redraw helper (clippy clean).
- **macOS app version**: align `VoxTerm.app` Info.plist to 1.0.21.

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
- **CLI flags**: consolidated into a single doc with voxterm and rust_tui sections, plus missing flags and log env vars.

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
- **Binary rename**: `voxterm` is now the only user-facing command (no `codex-overlay`).
- **Prompt log path**: configured via `--prompt-log` or `VOXTERM_PROMPT_LOG` (no default unless set).
- **Env cleanup**: Legacy overlay prompt env vars are no longer supported; use `VOXTERM_PROMPT_*`.
- **Docs/scripts**: update build/run instructions to use `voxterm`.

## [1.0.15] - 2026-01-29

### Fixes
- **Overlay build fix**: remove stray duplicate block that broke compilation in `voxterm` (source: `src/bin/voxterm/`).

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
- **Clippy cleanup in voxterm**: resolve collapsible-if, map_or, clamp, and question-mark lints under `-D warnings` (source: `src/bin/voxterm/`).

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
- **Verified code path**: latency_measurement binary uses identical code path as voxterm (same voice::start_voice_job → stt::Transcriber).

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
- **VoxTerm banner**: `start.sh` now uses the Rust launch banner from the legacy CLI.
- **Compact quickstart tables**: launch output shows quick controls + common commands in green tables.
- **Adaptive layout**: smaller banner + dual-color columns keep tables visible in shorter terminals.
- **Startup output test**: `scripts/tests/startup_output_test.sh` guards line widths.

### Simplified Install Flow (2026-01-23) - COMPLETE
- **New installer**: added `scripts/install.sh` plus `scripts/setup.sh install` to download the Whisper model, build the Rust overlay, and install a `voxterm` wrapper.
- **Overlay-first defaults**: `scripts/setup.sh` now defaults to `install` so it builds the Rust overlay by default.
- **Docs updated**: README + QUICK_START now point to `./scripts/install.sh` and `voxterm` for the simplest path.

### Rust-Only Docs + Launchers (2026-01-23) - COMPLETE
- **Docs sweep**: removed legacy CLI references from user-facing docs and the audit.
- **Launchers aligned**: `start.sh` and `scripts/setup.sh` now run overlay-only; Windows launcher points to WSL/macos/linux.
- **Backlog added**: `dev/active/BACKLOG.md` tracks follow-up work and open UX items.

### Overlay UX (2026-01-23) - COMPLETE
- **New hotkeys**: Ctrl+T toggles send mode (auto vs insert), Ctrl++/Ctrl+- adjust mic sensitivity in 5 dB steps.
- **Startup hints**: `start.sh` prints the key controls and common flag examples for non-programmers.

### Homebrew Runtime Fixes (2026-01-23) - COMPLETE
- **Prebuilt overlay reuse**: `start.sh` now uses `voxterm` from PATH when available, skipping builds in Homebrew installs.
- **User-writable model storage**: model downloads fall back to `~/.local/share/voxterm/models` when the repo/libexec is not writable.
- **Homebrew detection**: Homebrew installs always use the user model directory instead of libexec, even if libexec is writable.
- **Install wrapper safety**: skip existing global `voxterm` commands and prefer safe locations unless overridden.

### Rust Overlay Mode + Packaging (2026-01-22) - COMPLETE
- **Added Rust overlay mode**: new `voxterm` binary runs Codex in a PTY, forwards raw ANSI output, and injects voice transcripts as keystrokes.
- **Prompt-aware auto-voice**: prompt detection with idle fallback plus configurable regex overrides for auto-voice triggering.
- **Serialized output writer**: PTY output + status line rendering go through a single writer thread to avoid terminal corruption.
- **PTY passthrough improvements**: new raw PTY session that answers DSR/DA queries without stripping ANSI.
- **Resizing support**: SIGWINCH handling updates PTY size and keeps the overlay stable.
- **Startup/launcher updates**: `start.sh` now defaults to overlay, ensures a Whisper model exists, and passes `--whisper-model-path`; macOS app launcher now uses overlay mode.
- **Docs refresh**: new `ARCHITECTURE.md` with detailed Rust-only diagrams and flows; README expanded with install paths, commands, and Homebrew instructions.
- **Repo hygiene**: internal architecture/archive/reference directories are now ignored by git and removed from the tracked set.

### Project Cleanup + macOS Launcher (2026-01-11) - COMPLETE
- **Added macOS app launcher**: `VoxTerm.app` now in repo alongside `start.sh` and `start.bat` for cross-platform consistency.
- **Major project structure cleanup**:
  - Removed duplicate files from `rust_tui/` (CHANGELOG, docs/, screenshots, etc.)
  - Moved rust_tui test scripts to `rust_tui/scripts/`
  - Consolidated scripts: deleted redundant launchers (`run_tui.sh`, `launch_tui.py`, `run_in_pty.py`)
  - Moved benchmark scripts to `scripts/tests/`
  - Deleted legacy folders (`stubs/`, `tst/`)
  - Kept `voxterm.py` as legacy Python fallback
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
- Implemented the Phase 1 backend refactor: `CodexJobRunner`/`CodexJob` abstractions with bounded queues, `CodexCliBackend` PTY ownership, App wiring to Codex events, and new queue/tests (`cargo test --no-default-features`).
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
- Updated the new references (`quick_start.md`, `testing.md`, `python_legacy.md`) to reflect the current Rust pipeline (Ctrl+R voice key, `cargo run` workflow, native audio tests) and annotated the legacy plan in `dev/archive/MVP_PLAN_2024.md`.
- Added a concise root `README.md`, introduced the “You Are Here” section in `PROJECT_OVERVIEW.md`, renamed `docs/guides/` → `docs/references/` (`quick_start.md`, `testing.md`, `python_legacy.md`, `milestones.md`, `troubleshooting.md`), and archived superseded guides under `dev/archive/OBSOLETE_GUIDES_2025-11-12/`.
- Updated helper scripts (`rust_tui/test_performance.sh`, `test_voice.sh`, `simple_test.sh`, `final_test.sh`) to rely on `cargo run`, Ctrl+R instructions, and the shared `${TMPDIR}/voxterm_tui.log`.
- Extended `agents.md` with an end-of-session checklist so every workday records architecture notes, changelog entries, and the “You Are Here” pointer.
- Consolidated the CI/CD references into a single `docs/references/cicd_plan.md`, merging the previous implementation + dependency guides and archiving the superseded files under `dev/archive/OBSOLETE_REFERENCES_2025-11-12/`.
- Expanded `docs/references/cicd_plan.md` with appendices covering phase-by-phase scripts, tooling/dependency matrices, rollback/cost controls, and troubleshooting so it fully supersedes the archived references.
- Captured the latency remediation plan in `docs/audits/latency_remediation_plan_2025-11-12.md` and updated `PROJECT_OVERVIEW.md` to prioritize latency stabilization workstreams ahead of module decomposition and CI enhancements.
- Strengthened the latency plan with explicit Phase 2A/2B/3 naming, backpressure/frame/shutdown/fallback policies, and structured tracing + CI perf gate requirements so phase execution is unambiguous.
- Added production-grade detail to the latency plan: failure hierarchy, VAD safety rails, bounded resource budgets, observability schema, CI enforcement hooks, and a 15-day execution timeline.
- Updated `agents.md` so the latency requirements explicitly point to the Phase 2B specification (`docs/audits/latency_remediation_plan_2025-11-12.md`) and mandate adherence to the new resource/observability/CI rules.
- Documented the state machine, config/deployment profiles, and concurrency guardrails inside the latency plan so downstream work follows the same lifecycle semantics.
- Hardened `agents.md` with Scope/Non-goals, "Before You Start" instructions, condensed voice requirements referencing the latency plan, explicit doc-update rules, and a prominent end-of-session checklist.
- Recorded the readiness audit (`docs/audits/READINESS_AUDIT_2025-11-12.md`), summarized its findings in the architecture log, and captured the Phase 2A design (Earshot VAD, config surface, metrics, exit criteria) plus the immediate task list (perf_smoke CI, Python feature flag, module decomposition planning).


See `docs/audits/latency_remediation_plan_2025-11-12.md` for the complete latency specification.
