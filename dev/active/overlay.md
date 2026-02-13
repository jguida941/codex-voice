# Overlay Strategy Research (2026-02-13)

This document consolidates competitor analysis and overlay enhancement research
for VoxTerm's "voice + AI CLI" strategy. It is intended for product positioning
and roadmap prioritization.

## Plan status
- Reference research only (market/competitor + UX analysis).
- The single active execution plan is `dev/active/MASTER_PLAN.md`.
- Deferred work is tracked in `dev/deferred/`.

## Summary

- VoxTerm is strongest when users need terminal-native workflow control:
  PTY passthrough, transcript queueing while CLI is busy, prompt-aware auto-voice,
  and Codex+Claude support.
- The largest competitive pressure is from "voice everywhere" tools that also
  work in terminals but are not deeply PTY/session aware.
- **No competitor currently combines voice input with AI command generation
  in the terminal.** Warp has AI but no voice; Voxtype/Wispr have voice but no
  AI preprocessing. This is VoxTerm's biggest expansion opportunity.
- 2026 is trending as "Year of Voice" — OpenAI reorganizing for audio AI,
  "vibe coding" (voice + AI) going mainstream, terminal AI agents maturing.
- 5-10% of software engineers have RSI; accessibility-focused voice tooling
  for terminals is an underserved niche VoxTerm can own.

## Comparison Matrix — Direct Competitors

| Product | Terminal support | Engine / privacy | AI-CLI depth | Relative to VoxTerm |
|---|---|---|---|---|
| VoxTerm | Native PTY wrapper around CLI | Local Whisper; no cloud by default | Codex + Claude first-class; prompt-aware queue + send modes + HUD/settings | Deep terminal orchestration with explicit CLI-state handling |
| Dictto | Terminal-adjacent via hotkeys + Claude workflows | Markets local/on-device processing on Mac | Strong Claude Code integration and voice-agent mode | Strong for Claude-only workflows; weaker as a multi-CLI PTY layer |
| Fisper | Yes, includes terminal auto-submit mode | Local/on-device on Apple Silicon | System-wide dictation + cursor insertion | Lightweight and fast; less explicit CLI prompt/session orchestration |
| Speech2Type | Yes, injects into focused terminal app | Uses external API key flow (Deepgram) | Generic voice typing across apps | Simpler OSS path but less local/private and less CLI-aware |
| Ottex | Yes, dedicated terminal workflow page | BYO AI model/API key (cloud-first) | Broad app support + agentic shortcuts | Broader consumer workflow product; weaker local-first privacy posture |
| Remote Coder | Yes, oriented around SSH sessions | On-device ASR/TTS claims | Focused on remote Claude Code control | Strong remote Claude niche; not a Codex+Claude local overlay |
| Claude Quick Entry | Not terminal-native | Claude Desktop voice entry | Claude-only desktop quick actions | Useful companion, not a terminal overlay substitute |
| Voxtype | Pastes into any focused app (incl. terminals) | Local Whisper (Rust); offline | None — pure dictation, no AI preprocessing | Single-binary competitor; push-to-talk only, no PTY awareness or CLI state |
| Whis | CLI tool, not a terminal wrapper | Cloud (OpenAI, Mistral, Groq, Deepgram) or local Whisper | None — transcription utility | Cargo-installable but no terminal integration; useful only for one-off transcription |
| Whispertux | Simple GUI around whisper.cpp | Local Whisper | None | Linux-only GUI; not terminal-native at all |

## Comparison Matrix — Broader Landscape

These are not direct competitors but occupy adjacent space and inform feature
direction.

| Product | Category | Voice? | AI? | Terminal-native? | Notes |
|---|---|---|---|---|---|
| Warp AI | AI-powered terminal | **No** | GPT-powered NL→command | Full terminal replacement | Biggest gap: no voice input. If Warp adds voice, it threatens VoxTerm |
| shell-gpt (SGPT) | CLI AI assistant | **No** | GPT-4 NL→shell commands | Shell command tool | Popular (10K+ stars); voice would make it dangerous |
| aichat | Multi-LLM CLI | **No** | 20+ providers, RAG, agents | Chat REPL + shell assistant | Most flexible AI CLI; no voice |
| GitHub Copilot CLI | AI CLI assistant | **No** | `gh copilot suggest/explain` | Works in any terminal | GitHub ecosystem lock-in; no voice |
| Wispr Flow | Developer dictation | Cloud STT | AI formatting/editing | IDE plugins (Cursor, Windsurf, Replit) | 175+ WPM; strongest dev dictation tool but not terminal-specific |
| Talon Voice | Voice coding platform | Custom STT | No | IDE-focused (VS Code + Cursorless) | Gold standard for RSI users; steep learning curve, expensive, not terminal |
| Serenade | Voice coding | Custom STT | No | IDE-focused | Natural language commands; more accessible than Talon, still IDE-only |
| Cursorless | Structural voice editing | Via Talon | No | VS Code only | Spoken language for code navigation; amazing but narrow scope |
| SuperWhisper | macOS dictation | Local Whisper | Modes/formatting | System-wide | Polished UX; mode switching concept worth studying |

## Market Gaps — Where VoxTerm Can Dominate

VoxTerm sits at an unoccupied intersection. No tool today combines voice +
AI command generation + terminal-native orchestration:

```
                    Voice Input
                        |
          Voxtype ------+------- VoxTerm  <-- current position
          Wispr Flow    |           |
                        |           |  (no competitor here)
                        |           v
                    ----+---- Voice + AI Commands  <-- expansion target
                        |
           Warp AI -----+------- (no voice)
           shell-gpt    |
           aichat       |
                        |
                   AI Commands
```

### Unmet needs (no tool addresses these today)

1. **Voice → AI command generation in terminal** — Warp has AI (no voice);
   dictation tools have voice (no AI). VoxTerm can bridge both.
2. **Voice macros for terminal workflows** — VoiceMacro (100K+ users) proves
   demand for general apps; nothing exists for terminal-specific triggers.
3. **Real-time voice overlay for tmux/neovim** — overlay tools exist for
   general apps, none are terminal-aware.
4. **Hybrid voice+keyboard terminal tool** — research shows 3-4x productivity
   gains from hybrid input; Wispr Flow is general, not terminal-aware.
5. **Accessibility-focused terminal voice tool** — Talon/Serenade target IDEs;
   terminal users with RSI have no dedicated solution.

### Underserved audiences

- AI CLI power users (Codex, Claude Code, Aider daily drivers)
- Developers with RSI (5-10% of engineers)
- Terminal purists who refuse GUI IDEs
- Privacy-conscious developers who want local-only processing
- DevOps/SRE with repetitive terminal workflows (macro candidates)

## Feature Expansion Roadmap

### Phase 1 — Quick Wins (1-2 weeks)

**1. Voice Macros / Custom Triggers**
Users define voice shortcuts that expand to commands. Pattern-match against
transcripts before PTY injection. Per-project macro files supported.
```yaml
# .voxterm/macros.yaml
macros:
  "run tests": "cargo test --all-features"
  "deploy staging": "git push origin staging"
  "commit with message":
    template: "git commit -m '{TRANSCRIPT}'"
    mode: insert  # waits for remaining speech to fill template
```

**2. Command Mode vs. Dictation Mode**
Toggle between two voice modes via hotkey (e.g., Ctrl+D):
- **Command mode**: shell-aware vocabulary, auto-submit, abbreviation expansion
  (`"git co main"` → `git checkout main`)
- **Dictation mode**: natural language prose, full punctuation, no auto-submit

Inspired by SuperWhisper's mode-switching system.

**3. Transcript Preview/Edit Before Send**
Show what Whisper heard in a small overlay before injecting into PTY. Allow
arrow-key editing and voice corrections ("replace X with Y"). Catches errors
before they reach the AI CLI.

### Phase 2 — Differentiators (1-2 months)

**4. LLM-Powered Command Generation from Voice** ← biggest opportunity
Voice → Whisper transcription → LLM preprocessing → optimized command →
confirmation UI → PTY injection. Example flow:
```
User says: "find all TypeScript files changed this week with functions over 50 lines"
VoxTerm:
  1. Whisper transcribes natural language
  2. Local LLM (Ollama) or API (Claude/OpenAI) generates shell command
  3. Preview: find . -name "*.ts" -mtime -7 -exec ...
  4. User confirms [Y/n/edit]
  5. Injects into PTY
```
Add `--llm-assist` flag with provider config. Optional — keeps VoxTerm usable
without any cloud dependency.

**5. Voice Terminal Navigation**
Go beyond dictation to actual terminal control via voice:
- "scroll up" / "scroll down"
- "copy last output"
- "show last error"
- "run previous command"
- "clear screen"
- "explain this error" (capture output + send to AI backend)

Leverages existing PTY access to send control sequences.

**6. Persistent Config & Transcript History**
- Save preferences to `~/.config/voxterm/config.toml` (theme, thresholds,
  mode, macros, default backend)
- Searchable transcript history (`voxterm --history` or Ctrl+H in overlay)
- Session replay for debugging missed transcriptions

### Phase 3 — Advanced Features (2-3 months)

**7. Neovim / Tmux Integration**
- Neovim plugin: `:VoxTermStart`, voice in command mode, voice nav
- Tmux awareness: detect active pane, voice pane switching
- VS Code integrated terminal panel support

**8. Streaming STT / Real-Time Overlay**
- Show partial transcripts as Whisper processes (whisper_streaming approach)
- Floating overlay showing live transcription above terminal
- Per-word confidence highlighting (dim uncertain words)
- Adaptive model selection: tiny for short commands, small/medium for long
  dictation

**9. Accessibility Suite**
Target the 5-10% of developers with RSI:
- Voice health monitoring (warn on extended/strained usage)
- Fatigue detection (suggest breaks after X minutes)
- Quiet/whisper capture mode (low-volume environments)
- Shorthand vocabulary expansion (minimal syllables per command)
- Screen reader compatibility for all overlays

**10. Custom Vocabulary / Fine-Tuning**
- Per-project word lists (API names, variable naming conventions)
- Auto-learn from project files (scan identifiers in codebase)
- User-trainable corrections (persistent word substitution rules)
- Context-aware punctuation (code mode vs. documentation mode)

## Overlay UX Enhancement Audit (2026-02-13)

### Current runtime UI coverage
- 11 color themes with brand-specific palettes
- 3 HUD styles (Full, Minimal, Hidden) with responsive layout (25-120+ chars)
- Modular HUD system with 4 pluggable modules (Mode, Meter, Latency, Queue)
- 3 real-time audio viz modes (Ribbon, Dots, Heartbeat)
- 3 animations (pulsing record indicator, braille spinner, heartbeat)
- Interactive settings overlay (11 items, keyboard + mouse navigation)
- Clickable HUD buttons with mouse position tracking
- Help overlay, theme picker with live preview
- Startup splash with ASCII art + gradient treatment
- Mic calibration wizard
- Terminal capability auto-detection (TrueColor/256/ANSI16/None, NO_COLOR)

### Current runtime UI gaps
1. No streaming/live transcript preview
2. No transcript history UI
3. No voice macros UI
4. No AI command generation UI
5. No color gradients in runtime HUD rendering (splash-only treatment today)
6. No animation easing/tweening (frame-based only)
7. No spectrum analyzer or frequency visualization
8. No accessibility suite (screen reader/fatigue guidance)
9. No waveform peak detection
10. No toast notification system (inline status text only)

### Overall assessment
Current polish is strong but still short of a "premium" interaction layer.
The existing modular HUD architecture is a solid base for incremental upgrades.

## Overlay UX Enhancement Candidates (Research Intake)

These items are research-derived references. They are not execution authority by
themselves; implementation priority and status are tracked in `MASTER_PLAN.md`.

| Candidate | Complexity | Impact | Notes |
|---|---|---|---|
| TachyonFX animation system | Medium | High | Fade, dissolve, slide, color sweeps for panel/state transitions |
| Sparkline/chart telemetry widgets | Easy-Medium | High | Ratatui-native sparkline/gauge/chart for richer HUD metrics |
| Toast notifications | Medium | High | Auto-dismissing queue/error/success toasts with history |
| Command palette (fzf-style) | Medium | High | Searchable overlay command/action surface for discoverability |
| Siri/Alexa-style voice-state visuals | Medium | High | Distinct idle/listening/processing/done visuals |
| Adaptive/contextual HUD | Medium | Medium | Expand/shrink HUD by state with contextual badges |
| Session dashboard/stats panel | Medium | Medium | Session analytics with optional export |
| Multi-modal input panel | Medium-Hard | High | Hybrid voice + keyboard correction workflow |
| Fig-style autocomplete dropdown | Medium-Hard | Medium | Contextual macro/suggestion completion overlays |
| Block-based output model | Hard | High | Warp-style command/result blocks; major architecture shift |

### Priority recommendation

| # | Feature | Complexity | Impact | Why |
|---|---|---|---|---|
| 1 | TachyonFX animations | Medium | High | Immediate perceived quality lift with ratatui-native effects |
| 2 | Sparkline audio/latency telemetry | Easy | High | Uses built-in widgets and existing metrics streams |
| 3 | Toast notifications | Medium | High | Improves signal delivery over transient inline text |
| 4 | Command palette | Medium | High | High discoverability gain for growing control surface |
| 5 | Voice state visualization | Medium | High | Makes listening/transcribe states more legible |
| 6 | Adaptive HUD | Medium | Medium | Better information density with less idle clutter |
| 7 | Session dashboard | Medium | Medium | Exposes already-collected signals to users |
| 8 | Multi-modal input panel | Hard | High | Enables hybrid voice + keyboard correction flow |
| 9 | Fig-style autocomplete | Hard | Medium | Useful but positioning constraints in PTY wrapper |
| 10 | Block-based output | Hard | High | High upside but highest architecture risk |

## VoxTerm Current Strengths (Audit Summary, 2026-02-12)

- **Architecture**: Clean PTY passthrough; bounded crossbeam channels; serialized
  writer thread prevents output corruption; multi-backend registry.
- **Reliability**: 409 passing tests; panic-safe terminal restore; transcript
  queueing under busy CLI; Python fallback when native Whisper unavailable.
- **Audio pipeline**: CPAL recording → 16kHz resampling → VAD (earshot ML or
  simple threshold) → Whisper → text sanitization → PTY injection (~250ms).
- **UX**: 11 themes, 3 HUD styles, interactive settings overlay, mouse support,
  mic calibration tool, startup splash.
- **Distribution**: Homebrew tap, CI/CD (format + clippy + tests + mutation +
  perf + memory), macOS app launcher.
- **Documentation**: 6 user guides, 2 dev docs, 24 ADRs, full changelog.

### Known Gaps (from audit)

- Mutation score 4.41% (target 80%) — MP-015 in progress
- Gemini backend broken; Aider/OpenCode untested
- No persistent preferences or transcript history
- No streaming STT (full capture before transcription, ADR-0003)
- No Windows native support (WSL2 only)
- Legacy Codex-centric naming throughout codebase
- event_loop.rs is oversized (~82K LOC)
- No AI preprocessing layer for voice input

## VoxTerm Evidence (Repo)

- PTY + local Whisper + unchanged CLI output: `README.md`
- Local privacy claim and feature table: `README.md`
- Busy-CLI queue behavior and prompt fallback notes: `guides/USAGE.md`
- Voice mode, queue, backend, and HUD controls: `guides/CLI_FLAGS.md`
- PTY-only write model ("does not call Codex/Claude directly"): `QUICK_START.md`

## Market Evidence (External)

### Direct competitors / voice-to-text tools
- Dictto: https://dictto.app/
- Fisper: https://fisper.app/
- Speech2Type: https://www.speech2type.com/
- Speech2Type repo: https://github.com/gergomiklos/speech2type
- Ottex home: https://ottex.ai/
- Ottex apps: https://ottex.ai/apps/
- Ottex terminal page: https://ottex.ai/apps/terminal/
- Remote Coder: https://remotecoder.app/
- Claude Quick Entry: https://support.claude.com/en/articles/12626668-use-quick-entry-with-claude-desktop-on-mac
- Voxtype: https://voxtype.io/
- Whis (Cargo CLI): https://github.com/frankdierolf/whis
- Whispertux: https://github.com/cjams/whispertux
- SoupaWhisper (Linux SuperWhisper alt): https://www.ksred.com/soupawhisper-how-i-replaced-superwhisper-on-linux/

### AI terminal assistants (no voice — expansion targets)
- Warp AI: https://www.warp.dev/compare-terminal-tools/github-copilot-vs-warp
- shell-gpt (SGPT): https://github.com/TheR1D/shell_gpt
- aichat: https://github.com/sigoden/aichat
- GitHub Copilot CLI: https://leonardomontini.dev/copilot-cli-vs-warp-ai/
- AIAssist: https://github.com/mehdihadeli/AIAssist

### Voice coding tools (IDE-focused, not terminal)
- Talon Voice: https://github.com/talonvoice
- Cursorless: https://www.cursorless.org/docs/user/customization/
- Serenade: https://serenade.ai/
- Wispr Flow: https://wisprflow.ai/
- Oravo AI: https://oravo.ai/blog/voice-dictation-for-developers

### Voice macro / automation references
- VoiceMacro: https://www.voicemacro.net/
- SuperWhisper modes: https://superwhisper.com/docs/modes/switching-modes

### Whisper streaming / performance research
- whisper_streaming: https://github.com/ufal/whisper_streaming
- WhisperLive: https://github.com/collabora/WhisperLive
- Deepgram on Whisper streaming limits: https://deepgram.com/learn/why-enterprises-are-moving-to-streaming-and-why-whisper-can-t-keep-up

### Accessibility / RSI
- VoiceGrip research (5-10% RSI rate): https://www.researchgate.net/publication/44075430_VoiceGrip_A_Tool_for_Programming-by-Voice
- RSI voice care: https://rsi.org.au/index.php/treating-rsi/computing-by-voice/taking-care-of-your-voice/

### 2026 trends
- Year of Voice (2026): https://www.standard.net/lifestyle/home_and_family/2026/feb/10/tech-matters-is-this-the-year-of-voice/
- Vibe Coding (voice + AI): https://wisprflow.ai/vibe-coding
- AI terminal renaissance: https://instil.co/blog/ai-predictions-2026

### Overlay UX / TUI references
- TachyonFX crate: https://github.com/ratatui/tachyonfx
- TachyonFX ecosystem page: https://ratatui.rs/ecosystem/tachyonfx/
- TachyonFX docs: https://docs.rs/tachyonfx/latest/tachyonfx/
- Ratatui widget showcase: https://ratatui.rs/showcase/widgets/
- Sparkline example: https://ratatui.rs/examples/widgets/sparkline/
- Gauge example: https://ratatui.rs/examples/widgets/gauge/
- CLI viz reference: https://github.com/sam1am/cli-viz
- Warp output model reference: https://www.warp.dev/blog/2025-in-review
- Warp product reference: https://www.warp.dev/terminal
- Warp architecture/experience analysis: https://thenewstack.io/how-warp-went-from-terminal-to-agentic-development-environment/
- Fig autocomplete (historical): https://github.com/withfig/autocomplete
- Fig docs: https://fig.io/user-manual/autocomplete
- fzf project: https://github.com/junegunn/fzf
- Command palette UX pattern: https://uxpatterns.dev/patterns/advanced/command-palette
- Voice UI visual design references: https://www.smashingmagazine.com/2021/06/alternative-voice-ui-voice-assistants/
- Voice UI best practices: https://designlab.com/blog/voice-user-interface-design-best-practices
- Multimodal VUI design: https://www.parallelhq.com/blog/voice-user-interface-vui-design-principles
- Toast UX references: https://blog.logrocket.com/ux-design/toast-notifications/
- Carbon notification patterns: https://carbondesignsystem.com/patterns/notification-pattern/
- Productivity dashboard references: https://jellyfish.co/library/developer-productivity/dashboard/
- Productivity analytics references: https://getdx.com/

## Notes

- Some competitor details (especially CLI-state awareness) are inferred from
  public positioning and may need direct product testing for strict validation.
- Some overlay UX references are pattern-level inspiration and should be tested
  directly against VoxTerm's PTY passthrough model before implementation.
- The broader landscape table captures tools that do not directly compete
  today but could add voice (Warp, shell-gpt) or terminal support (Wispr,
  Talon) and become threats. Monitor quarterly.
- Phase 2 "LLM-Powered Command Generation" is the single highest-leverage
  feature — it occupies white space no competitor has claimed. Prioritize
  above all other expansion work.
- Accessibility (Phase 3) is both a moral imperative and a market opportunity;
  5-10% of developers is a large addressable audience with high willingness
  to pay for tools that work.
