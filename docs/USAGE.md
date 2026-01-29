# Usage

Codex Voice runs the Codex CLI in a real PTY and forwards raw ANSI output directly to your
terminal. You interact with Codex's native UI; the overlay only handles its own hotkeys and
injects voice transcripts as keystrokes. It does not parse slash commands or replace Codex's UI.

![Overlay Running](../img/overlay-running.png)

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+T` | Toggle send mode (auto vs insert) |
| `Ctrl+]` | Increase mic threshold +5 dB |
| `Ctrl+\` | Decrease mic threshold -5 dB (or `Ctrl+/`) |
| `Ctrl+Q` | Exit overlay |
| `Ctrl+C` | Forward to Codex |
| `Enter` | In insert mode, stop capture early and transcribe what was captured |

Insert send mode note: press Enter while recording to stop early and transcribe what was
captured. When the transcript appears, press Enter again to send it to Codex. If the Python
fallback is active, Enter cancels the capture instead.

| | |
|---|---|
| ![Recording](../img/recording.png) | ![Auto-voice](../img/auto-voice.png) |
| **Voice Recording** (`Ctrl+R`) | **Auto-voice Mode** (`Ctrl+V`) |

## Auto-voice vs send mode (two separate toggles)

- **Auto-voice (Ctrl+V)** controls when listening starts. When enabled, the overlay starts
  listening automatically after a prompt or idle period.
- **Send mode (Ctrl+T)** controls what happens after a transcript: auto sends a newline,
  insert leaves the text so you can edit or press Enter to send.
- It is normal to be in auto-voice and still press Enter. Enter is used to stop capture
  early in insert mode or to send the typed transcript.

## Auto-voice behavior

- Auto-voice stays enabled even when no speech is detected; press `Ctrl+V` to stop it.
- The status line keeps "Auto-voice enabled" visible while waiting.
- If Codex is busy, voice transcripts are queued and sent when the next prompt appears
  (status shows the queued count). If a prompt has not been detected yet, an idle timeout
  can still auto-send them. Queued transcripts are merged into a single message when flushed.

## Long dictation

Long dictation is chunked by `--voice-max-capture-ms` (default 30s, max 60s). Use insert mode
for continuous dictation while Codex is busy.

## More options

- Full CLI flag list: [CLI flags](CLI_FLAGS.md)
- Install details: [Installation](INSTALL.md)
- Troubleshooting: [Troubleshooting](TROUBLESHOOTING.md)
