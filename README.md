# Codex Voice Quick Start

**Work in Progress**

Minimal steps to get the Rust TUI running with voice capture.

## 1. Install Requirements
- Rust toolchain (stable 1.88+) – `rustup update`
- Codex CLI already configured on your machine
- Whisper GGML model (tiny/base/small) placed under `models/`

## 2. Build & Run
```bash
cd codex_voice/rust_tui

# Build once
cargo build --release

# Run with a model
cargo run --release -- \
  --seconds 5 \
  --whisper-model-path ../models/ggml-base.en.bin
```

## 3. Use the TUI
- `Ctrl+R` – start voice capture
- `Ctrl+V` – toggle auto-capture after each Codex reply
- `Enter` – send the prompt
- `Esc` – clear input
- `Ctrl+C` – exit

Logs land in `${TMPDIR}/codex_voice_tui.log` (tail the file if debugging).

## 4. Need More?
- Architecture & daily decisions: latest folder under `docs/architecture/YYYY-MM-DD/`
- Roadmap + “you are here”: `PROJECT_OVERVIEW.md`
- Procedures & references: `docs/references/`
- SDLC rules: `agents.md`
