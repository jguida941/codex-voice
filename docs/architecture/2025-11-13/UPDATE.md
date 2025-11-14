# Update Notes — 2025-11-14

Brainstorm space for upcoming improvements. Items here are drafts until folded into `agents.md` or the daily architecture log.

## Ideas in progress

- **Async runtime decision**  
  - Repo currently uses `std::thread::spawn` everywhere (no Tokio/smol/async-std).  
  - Latency targets (sub-second streaming) and concurrency (mic capture + WS streaming + Codex tokens) suggest we evaluate moving to Tokio for async I/O, timers, cancellation, backpressure-aware channels, and WebSocket support.  
  - Action: produce a proposal comparing “stay blocking threads” vs “adopt Tokio” with risks, perf expectations, and iOS embedding constraints.

- **Tokio-based voice core**  
  - Design a Rust async engine that handles inbound mic buffers, outbound streaming (reqwest/hyper WebSockets), LLM token dispatch, playback buffering, and backpressure.  
  - Core runs under `#[tokio::main]` and exposes APIs (start_voice_session, push_audio_frame, next_playback_chunk).  
  - Needs integration plan with existing `VoiceJob`/`CodexBackend` abstractions + CI tests for async pipelines.

- **Swift / UniFFI embedding**  
  - Target architecture: thin Swift UI shell + Rust async engine exported via UniFFI.  
  - Define FFI-safe APIs (start_voice_session, push_audio_frame(Vec<i16>), get_playback_chunk -> Option<Vec<i16>>) and error types.  
  - Decide on audio formats, threading rules, and lifecycle so iOS apps can embed the engine safely.

- **Remote IDE wrapper + iOS client plan**  
  - Components: `-cli` (Rust) wrapping Codex/Claude, `-server` relay (Axum + WebSocket) for NAT/push, SwiftUI client using UniFFI-exported Rust core.  
  - Session security: X25519 key exchange, AEAD (XChaCha20-Poly1305) per frame, QR-code session bootstrap, relay only forwards ciphertext.  
  - Protocol: JSON control frames (`session_open`, `start_stream`, `exec_cmd`) plus binary frames for audio (PCM/Opus) and token events.  
  - Infrastructure: Tokio runtime, tokio-tungstenite for streaming, opus encoder for audio compression, AEAD helpers via x25519-dalek + chacha20poly1305, UniFFI for Swift bindings.  
  - Roadmap steps: build encrypted happy-cli/WebSocket, add audio streaming and control channel, implement relay + APNs, integrate SwiftUI app, harden crypto/logging before TestFlight.

- **Unified Codex architecture (Happy-Coder style)**  
  - Four Rust-centric components: `codex-cli-wrapper` (CodexBackend, slash routing, PTY/API), `codex-voice-engine` (Tokio voice pipeline, STT streaming, latency metrics), `codex-sync-server` (Axum relay, encrypted session control), and `codex-mobile` (SwiftUI shell + Rust core via UniFFI).  
  - Repository layout: workspace root with one crate per component, shared governance docs in `docs/governance/AGENTMD.md`, daily architecture folders per module.  
  - Governance mapping: each subsystem follows AgentMD rules (design-before-code, daily architecture notes, module-level changelog). CI enforces updates to `PROJECT_OVERVIEW.md`, daily docs, and changelogs whenever a module changes.  
  - CodexBackend is the single integration point; all modules call it instead of touching Codex directly. Voice pipeline uses `CodexBackend::send_message`, sync server mediates control tokens, mobile client consumes streamed tokens/audio via encrypted WebSocket.  
  - Remote control rules: sync server tracks active controller (desktop vs phone); takeovers require explicit approval, desktop regains control via keypress, and mobile degrades gracefully when revoked.

- **Feature roadmap** — goal is to turn Codex into a true IDE+mobile platform, not just a CLI wrapper. For each bullet, we need design notes + approvals before coding.  
  - Slash-command suite: implement `/search`, `/define`, `/explain`, `/refactor`, `/test`, `/docs`, `/summarize`, `/commit`; goal = VS Code parity inside TUI.  
  - Autonomous task graph: goal = “Devin-lite” planning; break requests into tasks, run sequentially with checkpoints, stream status to desktop + phone.  
  - Remote-continue mode: goal = user can leave laptop; desktop keeps coding/tests running while phone receives logs, diffs, approvals via push/WebSocket.  
  - Multi-agent orchestration: goal = specialized Rust workers (coder/tester/docs/refactor) communicating over channels; mobile UI shows agent states.  
  - Code-sidecar watcher: goal = file watcher informs AI + phone when workspace changes, triggers automated explanations/tests.  
  - Stream-diff protocol: goal = send add/remove/modify deltas so mobile shows live diffs; needs binary framing + diff renderer.  
  - Sandboxing: goal = run generated code safely via Docker/WASM/firejail with approval gates for destructive actions.  
  - Async guidance: goal = document when we must use Tokio (streaming, multi-client control, remote sync) vs. when simple blocking code is acceptable.

- **Developer UX & workflow enhancements**  
  - Prompt presets/roles via `prompts.toml` (`/mode refactor_strict`, `/mode security_review`).  
  - Session templates (`/session save|load`) to capture working dir, model, prompt preamble.  
  - Inline code actions (`/accept`, `/split`, unified patch format) for applying AI diffs.  
  - Knowledge snippets directory + `/kb search` for project-specific guidance fed into Codex.  

- **Observability & debugging**  
  - Telemetry crate emitting latency histograms, error codes, queue depth (format: `ts|phase|event|fields`).  
  - TUI `/debug` panel showing live job state, queues, percentiles.  
  - Session replay logs (audio + tokens + commands) with `/replay <file>` to reproduce offline.  

- **Safety & policy guardrails**  
  - `commands.toml` defining safety levels (read-only vs code vs execution) with confirmation rules.  
  - Protected path patterns (infra/*, secrets.*) requiring override before destructive actions.  
  - Audit log entries (actor, files, before/after hashes) for every destructive action.  

- **Extensibility & plugins**  
  - User-defined slash commands via `plugin.toml` (script/HTTP/Rust handler).  
  - Per-project toolchain mapping so `/test`, `/build` run repo-specific commands.  

- **Offline / degraded mode**  
  - Local-only fallback (STT/TTS + note taking) when remote Codex unavailable.  
  - Job spooling: queue prompts offline, `/flush` to send later or drop selectively.  

- **Voice UX improvements**  
  - Configurable interaction modes (push-to-talk, auto VAD, hybrid) with sensitivity knobs.  
  - Barge-in support mapping voice/keypress to CodexBackend cancellation.  
  - Audio cues for listening start, request accepted, model thinking, error.  

- **Testing & CI enhancements**  
  - Golden transcript tests for key workflows; CI validates structural results, not exact tokens.  
  - Fault injection (dropped audio frames, partial WS messages) to stress voice/Codex state machines.  
  - Model/Codex version drift checks with tolerance thresholds per golden test.  

- **Governance & ergonomics**  
  - Per-module `AGENT_MODULE.md` capturing local rules (voice engine: no blocking on audio thread; sync server: authenticate/encrypt messages).  
  - Feature flags/staged rollout for experimental features (Tokio runtime, remote sync server, multi-agent planner).  

- **User-facing differentiators**  
  - Proof bundles tying diffs + tests + static checks + rationale; shareable on desktop/mobile.  
  - Change contracts and verification (refuse diffs that violate declared goals).  
  - Transcript-linked commits + time-machine review (click commit to see convo slice, proof bundle, tests).  
  - Guardrails mode & design debt tracker for architecture-first workflows.  
  - Connected console + commute mode for remote monitoring/approval when away from keyboard.  
  - Failure cards + crash drill-down flows for guided debugging.  
  - Shareable sessions + pair tokens to replay or co-drive coding sessions.  
  - Explain-my-diff + refactor playbooks to build institutional knowledge.  
*** End Patch
