# Usage Guide

This guide explains how to use VoxTerm for hands-free coding with the Codex CLI
or Claude Code. Experimental presets exist for Gemini, Aider, and OpenCode, but
Gemini is currently nonfunctional and Aider/OpenCode are untested.

![VoxTerm](../img/logo.svg)

## Contents

- [Quick Start](#quick-start)
- [How Voice Input Works](#how-voice-input-works)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Voice Modes Explained](#voice-modes-explained)
- [Common Tasks](#common-tasks)
- [Project Voice Macros](#project-voice-macros)
- [Understanding the Status Line](#understanding-the-status-line)
- [Starting with Custom Options](#starting-with-custom-options)
- [See Also](#see-also)

## Quick Start

**Already installed?** Here's how to start talking to the CLI:

1. **Launch**: Run `voxterm` in your project folder
2. **Speak**: Press `Ctrl+R`, say your request, then pause
3. **Done**: VoxTerm types your words into the terminal for you. In auto mode
   it also presses Enter; in insert mode you press `Enter` yourself when ready.

That's it! Read on for more control over how voice input works.

**Backend note:** By default, `voxterm` launches the Codex CLI.
To use Claude Code:
- `voxterm --claude`

If you have not logged in yet:
- `voxterm --login --codex`
- `voxterm --login --claude`

Experimental presets (untested):
- `voxterm --gemini` (currently not working)
- `voxterm --backend aider`
- `voxterm --backend opencode`

---

## How Voice Input Works

VoxTerm turns your voice into keystrokes. It does **not** talk to any AI
directly - it just types into the terminal, exactly like you would.

1. **Record** - you speak, VoxTerm listens until you stop
2. **Transcribe** - Whisper converts speech to text locally (nothing leaves your machine)
3. **Expand (optional)** - if `.voxterm/macros.yaml` matches your transcript trigger, VoxTerm expands it first
4. **Type** - the final text is typed into the terminal automatically

That's it. The only difference between the two send modes is what happens
after the text is typed:

- **Auto** - VoxTerm also presses Enter, so the CLI processes it right away
- **Insert** - the text appears on the command line and you press `Enter`
  yourself (handy if you want to review or edit first)

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
| `Enter` | **Send/Stop** - In insert mode: stop recording early, or press `Enter` to send typed text |
| `Ctrl+C` | Forward interrupt to CLI |
| `Ctrl+Q` | **Quit** - Exit the overlay |

**Tip**: `Ctrl+/` also works for decreasing threshold (same as `Ctrl+\`).
Use **Left/Right** to move HUD button focus and **Enter** to activate the focused button.

---

## Settings Menu

Press `Ctrl+O` to open the settings overlay.
Navigate with **↑/↓**, adjust values with **←/→**, and press **Enter** to toggle
or activate the selected row. `Esc` closes the menu.

![Settings Menu](https://raw.githubusercontent.com/jguida941/voxterm/master/img/settings.png)

The menu surfaces the most common controls (auto-voice, send mode,
macros, mic sensitivity, theme) plus backend and pipeline
info.

It also lets you configure:
- **HUD style**: Full, Minimal, or Hidden
- **Right-side panel**: Off, Ribbon, Dots, Heartbeat (shown in Full and Minimal HUD)
- **Anim only**: Animate the right panel only while recording
- **Mouse**: Toggle HUD button clicks (on by default)

When Mouse is enabled, you can click HUD buttons and overlay controls.
Left/Right selects a HUD button and Enter activates it (even if Mouse is OFF).

---

## Voice Modes

Three controls shape voice behavior:

- **Auto-voice** (`Ctrl+V`) - when ON, VoxTerm listens automatically. When OFF, you press `Ctrl+R` each time.
- **Send mode** (`Ctrl+T`) - **auto** types your words and presses Enter. **Insert** types your words but lets you press Enter yourself.
- **Macros** (Settings -> Macros) - **ON** applies `.voxterm/macros.yaml` expansions before injection. **OFF** injects raw transcripts.

### Auto-voice × send mode combinations

| Auto-voice | Send mode | How it works |
|------------|-----------|--------------|
| Off | Auto | You press `Ctrl+R` to record. Text is typed + Enter is pressed for you. |
| Off | Insert | You press `Ctrl+R` to record. Text is typed. You press `Enter` when ready. |
| On | Auto | Just start talking. Text is typed + Enter is pressed for you. |
| On | Insert | Just start talking. Text is typed. You press `Enter` when ready. |

Macros toggle is orthogonal to this table:
- **ON**: macros can expand transcripts before injection.
- **OFF**: raw transcript is injected as-is.

### Tips

- **Enter during recording** (insert mode): stops recording early so it
  transcribes faster. Press `Enter` again to send.
- **Auto-voice ON** keeps listening after each transcript - you never need
  to press `Ctrl+R`.
- **When the CLI is busy**: VoxTerm waits, then types when the prompt returns.
- **Prompt detection**: if auto-voice doesn't re-trigger after the CLI
  finishes, it falls back to an idle timer. Set `--prompt-regex` if your
  prompt is unusual (especially with Claude).

### Long dictation (auto-voice + insert)

Each recording chunk is 30 seconds by default (max 60s via
`--voice-max-capture-ms`). With auto-voice + insert mode you can speak
continuously - each chunk is transcribed and typed, and auto-voice
immediately starts a new recording. Press `Enter` when you're done to send
everything.

---

## Common Tasks

### Adjust microphone sensitivity

If the mic picks up too much background noise or misses your voice:

- `Ctrl+]` - Less sensitive (raise threshold, ignore quiet sounds)
- `Ctrl+\` - More sensitive (lower threshold, pick up quieter sounds)

The status line shows the current threshold (e.g., "Mic sensitivity: -35 dB").
Hotkey range: -80 dB (very sensitive) to -10 dB (less sensitive). Default: -55 dB.
The CLI flag accepts a wider range (-120 dB to 0 dB).

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

### Tune auto-voice timing

```bash
# Idle time before auto-voice starts listening
voxterm --auto-voice-idle-ms 1200

# Idle time before queued transcripts flush
voxterm --transcript-idle-ms 250
```

### Notification sounds

```bash
# Enable all sounds
voxterm --sounds

# Only completion or error beeps
voxterm --sound-on-complete
voxterm --sound-on-error
```

---

## Project Voice Macros

You can define project-local voice triggers in `.voxterm/macros.yaml`.
VoxTerm loads this file from your current working directory and expands
matching transcripts before typing into the CLI.

Example:

```yaml
macros:
  run tests: cargo test --all-features
  commit with message:
    template: "git commit -m '{TRANSCRIPT}'"
    mode: insert
```

Rules:
- Trigger matching is case-insensitive and ignores repeated spaces.
- Template macros can consume extra spoken words via `{TRANSCRIPT}`.
- `mode` is optional and can be `auto` or `insert`.
- Macros are applied only when **Settings -> Macros = ON**.

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
- Optional right-side panel (Ribbon / Dots / Heartbeat) if enabled in Settings
- Compact telemetry chips (latency trend, queue, meter) that adapt to width/state

Latency badge behavior:
- When available, latency is shown as post-capture processing time (mainly STT).
- Recording duration while you speak is shown separately.
- If latency metrics are incomplete, VoxTerm hides the latency badge rather than
  showing an unreliable value.

When recording or processing, the mode label includes a pipeline tag
(e.g., `REC R` or `… PY`).

Visual updates:
- VoxTerm keeps bounded telemetry history for meter/latency and renders sparkline-style trends in compact HUD space.
- Short state-change pulse markers (`✦`, `•`, `·`) appear briefly when key mode/state changes occur.
- Compact HUD modules adapt by context:
  - recording: meter + latency + queue (space permitting)
  - backend busy: queue + latency
  - idle: latency-focused minimal telemetry

| Status | Meaning |
|--------|---------|
| `Auto-voice enabled` | Listening will start when the CLI is ready |
| `Listening Manual Mode (Rust)` | Recording now (you pressed Ctrl+R) |
| `Processing …` | Transcribing your speech (spinner updates) |
| `Transcript ready (Rust)` | Text typed into the terminal (auto mode also presses Enter) |
| `Transcript ready (Rust, macro 'run tests')` | A voice macro trigger matched and expanded before injection |
| `Macros: OFF` | Macro expansion disabled; transcripts are injected unchanged |
| `No speech detected` | Recording finished but no voice was heard |
| `Transcript queued (2)` | 2 transcripts waiting for the CLI to be ready |
| `Mic sensitivity: -35 dB` | Threshold changed |

"Rust" means fast native transcription. "Python" means fallback mode (slower but more compatible).

### Themes

Press `Ctrl+Y` to open the theme picker:

![Theme Picker](https://raw.githubusercontent.com/jguida941/voxterm/master/img/theme-picker.png)

Use ↑/↓ to move and Enter to select, or type the theme number.
With Mouse enabled (on by default), click a theme row to select it and click [×]
close to exit.

Available themes:
- `chatgpt`
- `claude`
- `codex`
- `coral`
- `catppuccin`
- `dracula`
- `nord`
- `tokyonight`
- `gruvbox`
- `ansi` (16-color)
- `none`

Theme tips:
- `voxterm --theme catppuccin` to start with a specific theme.
- If `--theme` is not set, VoxTerm picks a backend default (Claude → `claude`,
  Codex → `codex`, others → `coral`).
- `voxterm --no-color` or `NO_COLOR=1` to disable colors entirely.

### HUD Styles

For users who prefer less UI clutter, VoxTerm offers three HUD styles:

| Style | Flag | Description |
|-------|------|-------------|
| **Full** | (default) | 4-row banner with borders, shortcuts, and detailed info |
| **Minimal** | `--hud-style minimal` or `--minimal-hud` | Single-line strip |
| **Hidden** | `--hud-style hidden` | `VoxTerm` launcher row with `Ctrl+U` hint when idle; shows dim `REC` while recording |

Examples of the Minimal strip: `◉ AUTO · Ready`, `● REC · -55dB`.

When Mouse is enabled, Minimal HUD shows a [back] button on the right to return
to Full. If a right-panel mode is enabled, Minimal HUD also shows a compact
telemetry visualization chip (`Ribbon`, `Dots`, or `Heartbeat`).
Idle status text in Minimal HUD is intentionally compact to keep layout stable:
`Ready`, `Queued N`, `Warning`, or `Error`.

Minimal HUD (recording example):

![Minimal HUD](https://raw.githubusercontent.com/jguida941/voxterm/master/img/minimal-hud.png)

Hidden HUD (idle example):

![Hidden HUD](https://raw.githubusercontent.com/jguida941/voxterm/master/img/hidden-hud.png)

```bash
# Minimal HUD - just a colored mode indicator
voxterm --minimal-hud

# Hidden HUD - launcher row while idle, dim REC indicator while recording
voxterm --hud-style hidden
```

You can also change HUD style at runtime via the settings menu (`Ctrl+O`).

Preview tips:
- When a transcript completes, a short preview snippet appears in quotes for a few seconds.
- During recording, the status line shows a live waveform and the current dB level.
- The live dB display is clamped to a `-60dB` floor to avoid misleading extreme values.
- While backend output is streaming, HUD/settings interactions should remain responsive on current builds.

---

## Starting with Custom Options

Common startup configurations:

```bash
# Use Claude Code
voxterm --claude

# Hands-free capture + auto-send
voxterm --auto-voice

# Hands-free with insert mode (manual Enter send)
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
