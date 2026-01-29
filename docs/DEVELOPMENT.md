# Development

## Project structure

```
codex-voice/
├── Codex Voice.app/     # macOS double-click launcher
├── QUICK_START.md       # Fast setup and commands
├── docs/
│   ├── ARCHITECTURE.md    # Architecture diagrams and data flow
│   ├── CHANGELOG.md       # Release history
│   ├── CLI_FLAGS.md       # Full CLI and env reference
│   ├── DEVELOPMENT.md     # Build/test workflow
│   ├── INSTALL.md         # Install options and PATH notes
│   ├── TROUBLESHOOTING.md # Common issues and fixes
│   └── USAGE.md           # Controls and overlay behavior
├── img/                 # Screenshots
├── rust_tui/            # Rust overlay + voice pipeline
│   └── src/
│       ├── main.rs      # Entry point
│       ├── codex.rs     # Provider backend
│       ├── voice.rs     # Voice capture orchestration
│       ├── audio.rs     # CPAL recording, VAD
│       ├── audio/
│       │   └── recorder.rs # CPAL device capture and resample
│       ├── mic_meter.rs # Ambient/speech level sampler
│       ├── stt.rs       # Whisper transcription
│       └── pty_session.rs # PTY wrapper
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

# Mutation tests (CI enforces 80% minimum score)
cd rust_tui && cargo mutants --timeout 300 -o mutants.out
python3 ../scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80
```
