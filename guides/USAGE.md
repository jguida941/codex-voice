# Usage Guide

This guide explains how to use VoxTerm for hands-free coding with the Codex CLI
or Claude Code.

![Overlay Running](https://raw.githubusercontent.com/jguida941/voxterm/master/img/hero.png)

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

1. **Launch**: Run `voxterm` in your project folder
2. **Speak**: Press `Ctrl+R`, say your request, then pause, it sends automatically
3. **Done**: Your words appear as text and the CLI responds

That's it! Read on for more control over how voice input works.

**Backend note:** By default, `voxterm` launches the Codex CLI. To use Claude Code:
- `voxterm --claude`

**Gemini status:** Gemini CLI support is in works and not yet supported.

---

## How Voice Input Works

When you speak, VoxTerm:
1. Records your voice until you stop talking (silence detection)
2. Transcribes it to text using Whisper (runs locally, nothing sent to the cloud)
3. Types that text into the active CLI (Codex by default) and optionally presses Enter for you

![Recording Mode](https://raw.githubusercontent.com/jguida941/voxterm/master/img/recording.png)

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
| `Ctrl+O` | **Settings** - Open the settings menu (use ↑↓←→ + Enter) |
| `Ctrl+U` | **HUD style** - Cycle Full → Minimal → Hidden |
| `Ctrl+]` | **Threshold up** - Make mic less sensitive (+5 dB) |
| `Ctrl+\` | **Threshold down** - Make mic more sensitive (-5 dB) |
| `?` | **Help** - Show shortcut help overlay |
| `Enter` | **Send/Stop** - In insert mode: stop recording early, or send typed text |
| `Ctrl+C` | Forward interrupt to CLI |
| `Ctrl+Q` | **Quit** - Exit the overlay |

**Tip**: `Ctrl+/` also works for decreasing threshold (same as `Ctrl+\`).

---

## Settings Menu

Press `Ctrl+O` to open the settings overlay. Navigate with **↑/↓**, adjust values with **←/→**, and press **Enter** to toggle or activate the selected row. `Esc` closes the menu.

![Settings Menu](https://raw.githubusercontent.com/jguida941/voxterm/master/img/settings.png)

The menu surfaces the most common controls (auto-voice, send mode, mic sensitivity, theme) along with backend/pipeline info.
It also lets you configure:
- **HUD style**: Full (default banner), Minimal (single word), or Hidden (no HUD unless recording)
- **Right-side panel**: Off / Ribbon / Dots / Chips
- **Anim only**: Whether the right panel animates only while recording

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
- **Prompt detection fallback**: if auto-voice does not start after the CLI finishes, it will fall back to an idle timer; set `--prompt-regex` if your prompt is unusual (especially with Claude).
- **When the CLI is busy**: transcripts queue (manual or auto) and send when the next prompt appears (status shows the queued count). If a prompt is not detected, the queue will auto-send after output has been idle for the transcript idle timeout.
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
Range: -80 dB (very sensitive) to -10 dB (less sensitive). Default: -55 dB.

**Tip**: Run `voxterm --mic-meter` to measure your environment and get a suggested threshold.

### Check which audio device is being used

```bash
voxterm --list-input-devices
```

To use a specific device:
```bash
voxterm --input-device "MacBook Pro Microphone"
```

### Run diagnostics

```bash
voxterm --doctor
```

Prints terminal capabilities, log paths, and audio device info without starting the overlay.

---

## Understanding the Status Line

The bottom of your terminal shows the current state:

Example layout:
`◉ AUTO │ -35dB │ Auto-voice enabled   Ctrl+R rec  Ctrl+V auto`

Sections (left to right):
- Mode indicator (auto/manual/idle)
- Mic sensitivity in dB
- Status message (recording adds a live waveform + dB readout)
- Shortcut hints (on wide terminals)
- Optional right-side panel (Ribbon / Dots / Chips) if enabled in Settings

When recording/processing, the mode label includes a pipeline tag (e.g., `REC R` or `… PY`).

| Status | Meaning |
|--------|---------|
| `Auto-voice enabled` | Listening will start when the CLI is ready |
| `Listening Manual Mode (Rust)` | Recording now (you pressed Ctrl+R) |
| `Processing …` | Transcribing your speech (spinner updates) |
| `Transcript ready (Rust)` | Text sent to the CLI |
| `No speech detected` | Recording finished but no voice was heard |
| `Transcript queued (2)` | 2 transcripts waiting for the CLI to be ready |
| `Mic sensitivity: -35 dB` | Threshold changed |

"Rust" means fast native transcription. "Python" means fallback mode (slower but more compatible).

### Themes

Press `Ctrl+Y` to open the theme picker:

![Theme Picker](https://raw.githubusercontent.com/jguida941/voxterm/master/img/theme-picker.png)

Available themes: **chatgpt**, **claude**, **codex**, **coral**, **catppuccin**, **dracula**, **nord**, **ansi** (16-color), **none**.

- `voxterm --theme catppuccin` to start with a specific theme.
- If `--theme` is not set, VoxTerm picks a backend default (Claude → `claude`, Codex → `codex`, others → `coral`).
- `voxterm --no-color` or `NO_COLOR=1` to disable colors entirely.

### HUD Styles

For users who prefer less UI clutter, VoxTerm offers three HUD styles:

| Style | Flag | Description |
|-------|------|-------------|
| **Full** | (default) | 4-row banner with borders, shortcuts, and detailed info |
| **Minimal** | `--hud-style minimal` or `--minimal-hud` | Single-line strip (e.g., `◉ AUTO · Ready`, `● REC · -55dB`) |
| **Hidden** | `--hud-style hidden` | Blank row when idle; shows `REC` while recording |

Minimal HUD (recording example):

![Minimal HUD](https://raw.githubusercontent.com/jguida941/voxterm/master/img/minimal-hud.png)

```bash
# Minimal HUD - just a colored mode indicator
voxterm --minimal-hud

# Hidden HUD - nothing until you record
voxterm --hud-style hidden
```

You can also change HUD style at runtime via the settings menu (`Ctrl+O`).

Preview tips:
- When a transcript completes, a short preview snippet appears in quotes for a few seconds.
- During recording, the status line shows a live waveform and the current dB level.

---

## Starting with Custom Options

Common startup configurations:

```bash
# Use Claude Code
voxterm --claude

# Fully hands-free (auto-voice + auto-send)
voxterm --auto-voice

# Hands-free with review before sending
voxterm --auto-voice --voice-send-mode insert

# Specific microphone
voxterm --input-device "USB Microphone"

# Custom sensitivity
voxterm --voice-vad-threshold-db -35

# Force Whisper language
voxterm --lang en

# Enable notification sounds
voxterm --sounds
```

---

## See Also

| Document | What it covers |
|----------|----------------|
| [CLI Flags](CLI_FLAGS.md) | Complete list of all command-line options |
| [Installation](INSTALL.md) | Setup instructions for macOS, Linux, WSL |
| [Troubleshooting](TROUBLESHOOTING.md) | Common problems and solutions |
| [Architecture](../dev/ARCHITECTURE.md) | How the system works internally |
