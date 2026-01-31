# Usage Guide

This guide explains how to use Codex Voice for hands-free coding with the Codex CLI
(or another AI CLI via `--backend`).

![Overlay Running](../img/overlay-running.png)

## Contents

- [Quick Start](#quick-start)
- [How Voice Input Works](#how-voice-input-works)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Voice Modes Explained](#voice-modes-explained)
- [Common Tasks](#common-tasks)
- [Understanding the Status Line](#understanding-the-status-line)
- [Starting with Custom Options](#starting-with-custom-options)
- [See Also](#see-also)

## Quick Start

**Already installed?** Here's how to start talking to the CLI:

1. **Launch**: Run `codex-voice` in your project folder
2. **Speak**: Press `Ctrl+R`, say your request, then pause, it sends automatically
3. **Done**: Your words appear as text and the CLI responds

That's it! Read on for more control over how voice input works.

**Backend note:** By default, `codex-voice` launches the Codex CLI. To target another AI CLI,
pass `--backend` (for example `--backend claude` or `--backend gemini`). You can also pass a
custom command string.

---

## How Voice Input Works

When you speak, Codex Voice:
1. Records your voice until you stop talking (silence detection)
2. Transcribes it to text using Whisper (runs locally, nothing sent to the cloud)
3. Types that text into the active CLI (Codex by default) and optionally presses Enter for you

You control **when** recording starts and **what happens** after transcription.

---

## Keyboard Shortcuts

All shortcuts in one place:

| Key | What it does |
|-----|--------------|
| `Ctrl+R` | **Record** - Start voice capture (manual mode) |
| `Ctrl+V` | **Voice toggle** - Turn auto-voice on/off |
| `Ctrl+T` | **Typing mode** - Switch between auto-send and insert mode |
| `Ctrl+Y` | **Theme picker** - Choose a status line theme |
| `Ctrl+]` | **Threshold up** - Make mic less sensitive (+5 dB) |
| `Ctrl+\` | **Threshold down** - Make mic more sensitive (-5 dB) |
| `?` | **Help** - Show shortcut help overlay |
| `Enter` | **Send/Stop** - In insert mode: stop recording early, or send typed text |
| `Ctrl+C` | Forward interrupt to CLI |
| `Ctrl+Q` | **Quit** - Exit the overlay |

**Tip**: `Ctrl+/` also works for decreasing threshold (same as `Ctrl+\`).

---

## Voice Modes Explained

Two toggles control how voice works. Use `Ctrl+V` for auto-voice, `Ctrl+T` for send mode.
If auto-voice is off, press `Ctrl+R` to start recording.

### Mode chart (all combinations)

| Auto-voice (`Ctrl+V`) | Send mode (`Ctrl+T`) | How you start | After you stop talking | Best for |
|-----------------------|----------------------|---------------|------------------------|----------|
| Off | Auto | Press `Ctrl+R` | Transcribes and sends immediately | Quick commands, precise timing |
| Off | Insert | Press `Ctrl+R` | Transcribes, waits - press `Enter` to send | Edit before sending |
| On | Auto | Just speak (auto starts after prompt/idle) | Transcribes and sends immediately | Fully hands-free |
| On | Insert | Just speak (auto starts after prompt/idle) | Transcribes, waits - press `Enter` to send | Hands-free + review / long dictation |

**Notes**
- **Insert mode Enter**: press `Enter` while recording to stop early, then press `Enter` again to send.
- **Auto-voice status**: "Auto-voice enabled" means it is waiting to listen; the mic is not recording yet.
- **Prompt detection fallback**: if auto-voice does not start after the CLI finishes, it will fall back to an idle timer; set `--prompt-regex` if your prompt is unusual or you are using a non-Codex backend.
- **When the CLI is busy**: transcripts queue and send when the next prompt appears (status shows the queued count). If a prompt has not been detected yet, an idle timeout can still auto-send them. Queue flushing is currently unreliable and tracked in the [master plan](active/MASTER_PLAN.md).
- **Python fallback**: if the Python pipeline is active, pressing `Enter` while recording cancels the capture instead of stopping early.

### Long dictation (auto-voice + insert)

Each recording chunk is 30 seconds by default (max 60s via `--voice-max-capture-ms`). With auto-voice
and insert mode, you can speak continuously:

1. Turn on auto-voice (`Ctrl+V`) and set send mode to insert (`Ctrl+T`).
2. Start speaking. After 30 seconds, the chunk is transcribed and appears on screen.
3. Auto-voice immediately starts a new recording. Keep talking.
4. Repeat as long as you want. Each chunk gets added to your message.
5. Press `Enter` when done to send everything.

---

## Common Tasks

### Adjust microphone sensitivity

If the mic picks up too much background noise or misses your voice:

- `Ctrl+]` - Less sensitive (raise threshold, ignore quiet sounds)
- `Ctrl+\` - More sensitive (lower threshold, pick up quieter sounds)

The status line shows the current threshold (e.g., "Mic sensitivity: -35 dB").
Range: -80 dB (very sensitive) to -10 dB (less sensitive). Default: -40 dB.

**Tip**: Run `codex-voice --mic-meter` to measure your environment and get a suggested threshold.

### Check which audio device is being used

```bash
codex-voice --list-input-devices
```

To use a specific device:
```bash
codex-voice --input-device "MacBook Pro Microphone"
```

---

## Understanding the Status Line

The bottom of your terminal shows the current state:

Example layout:
`◉ AUTO │ Rust │ -35dB │ Auto-voice enabled   Ctrl+R rec  Ctrl+V auto`

Sections (left to right):
- Mode indicator (auto/manual/idle)
- Pipeline (Rust or Python)
- Mic sensitivity in dB
- Status message (recording adds a live waveform + dB readout)
- Shortcut hints (on wide terminals)

| Status | Meaning |
|--------|---------|
| `Auto-voice enabled` | Listening will start when the CLI is ready |
| `Listening Manual Mode (Rust pipeline)` | Recording now (you pressed Ctrl+R) |
| `Listening Auto Mode (Rust pipeline)` | Recording now (auto-triggered) |
| `Processing …` | Transcribing your speech (spinner updates) |
| `Transcript ready (Rust pipeline)` | Text sent to the CLI |
| `No speech detected` | Recording finished but no voice was heard |
| `Transcript queued (2)` | 2 transcripts waiting for the CLI to be ready |
| `Mic sensitivity: -35 dB` | Threshold changed |

"Rust pipeline" means fast native transcription. "Python pipeline" means fallback mode (slower but more compatible).

Theme tips:
- `codex-voice --theme catppuccin` to change the status line palette.
- `codex-voice --no-color` or `NO_COLOR=1` to disable colors.
- `Ctrl+Y` opens the theme picker overlay while the app is running.

Preview tips:
- When a transcript completes, a short preview snippet appears in quotes for a few seconds.
- During recording, the status line shows a live waveform and the current dB level.

---

## Starting with Custom Options

Common startup configurations:

```bash
# Use a different AI CLI backend
codex-voice --backend claude

# Fully hands-free (auto-voice + auto-send)
codex-voice --auto-voice

# Hands-free with review before sending
codex-voice --auto-voice --voice-send-mode insert

# Specific microphone
codex-voice --input-device "USB Microphone"

# Custom sensitivity
codex-voice --voice-vad-threshold-db -35

# Force Whisper language
codex-voice --lang en

# Enable notification sounds
codex-voice --sounds
```

---

## See Also

| Document | What it covers |
|----------|----------------|
| [CLI Flags](CLI_FLAGS.md) | Complete list of all command-line options |
| [Installation](INSTALL.md) | Setup instructions for macOS, Linux, WSL |
| [Troubleshooting](TROUBLESHOOTING.md) | Common problems and solutions |
| [Architecture](dev/ARCHITECTURE.md) | How the system works internally |
