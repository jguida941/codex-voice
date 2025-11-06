Here’s the full plan, end to end, based on everything we just hammered out.

0. Goal in one sentence

Build your tool (not OpenAI’s) that:
mic → wav → STT (Whisper/whisper.cpp) → text → call existing OpenAI Codex CLI → show result
and later wrap it in a Rust terminal UI.

⸻

1. MVP (Python, 1 file)

Why: fastest path, proves the pipeline, no source from OpenAI needed.

File: codex_voice.py

Does:
    1.    record N seconds from mic (Python sounddevice → audio.wav)
    2.    run STT via external command:
    •    whisper audio.wav --language en --model small --output_format txt
    •    or whisper.cpp …
    3.    take transcript, call Codex CLI:
    •    try codex "<transcript>" (arg mode)
    •    if that fails, try piping to stdin
    4.    print Codex output
    5.    optional: say "Codex result ready" on macOS

This proves: “I don’t have their source but I can still drive their CLI.”

⸻

2. Make it configurable

Still Python, same file, just add flags:

python codex_voice.py \
  --seconds 5 \
  --whisper-cmd whisper \
  --whisper-model small \
  --lang en \
  --codex-cmd codex

So if they rename their CLI, you don’t care.

⸻

3. Lock in interfaces (this is the “smart” part)

Define 3 clean functions in that Python file:

record_wav(path: str, seconds: int) -> None
transcribe(path: str, whisper_cmd: str, lang: str, model: str) -> str
call_codex_auto(prompt: str, codex_cmd: str) -> str

Everything else is glue.
This makes it trivial to port later to Rust/Go because you know the 3 ops you must implement.

⸻

4. Add “voice mode” behavior concept

You described a nice UX:
    •    trigger voice with a flag (--speak) or a command (/speak)
    •    show [REC] while recording
    •    VAD → finish → dump text into input
    •    Enter sends to Codex
    •    any other key stops recording so you can edit

We can’t do the full terminal interactivity cleanly in plain Python without extra libs, so we implement the behavior now (record → transcribe → edit → send) and move the nice UI to Rust later.

So in Python MVP:
    •    record
    •    show transcript
    •    ask: “press Enter to send to Codex or edit text:”
    •    send

That maps 1:1 to the Rust TUI version you want.

⸻

5. Rust TUI wrapper (Phase 2)

Now that Python MVP proves the pipeline, write the real terminal UI in Rust.

Crates:
    •    ratatui (or tui) for layout
    •    crossterm for key events
    •    anyhow for errors

Rust app state:

struct AppState {
    input: String,           // current line
    output: Vec<String>,     // scrollback from codex
    voice_enabled: bool,     // like your /speak
    lang: String,            // pass to STT
}

Flow in Rust:
    1.    draw UI: top = output, bottom = input
    2.    key handler:
    •    if user types /speak → toggle voice_enabled = true
    •    if voice_enabled:
    •    call external STT command
    •    set state.input = transcript
    •    wait for Enter
    •    on Enter:
    •    call Codex CLI via Command
    •    push stdout into state.output
    •    if voice_enabled, keep it on for next round (your “stay in flow” idea)

Important: Rust never does STT itself — it just does:

Command::new("whisper")
   .args([...])
   .output()

So you keep the same STT as Python.

⸻

6. Audio in Rust (macOS)

To avoid fighting CoreAudio, just shell out to ffmpeg/sox from Rust/Python:

ffmpeg -f avfoundation -i ":0" -t 5 out.wav

Your Rust code runs that, then runs STT on out.wav.
Later, if you want, you replace with a Rust audio crate.

⸻

7. Distribution plan
    •    dev mode: python codex_voice.py ...
    •    your machine only: put it in ~/bin and alias it:

alias codex="python ~/bin/codex_voice.py --seconds 5 --codex-cmd codex"


    •    public (later): build the Rust TUI into a single binary, tell users:
    1.    install whisper/whisper.cpp
    2.    install OpenAI Codex CLI
    3.    run codex-voice

This avoids ever shipping OpenAI’s stuff yourself.

⸻

8. Money / sellable angle

Because your tool only calls their CLI and doesn’t include it, you can sell:
    •    the Rust TUI
    •    the “always listening” flow
    •    the editor-friendly clipboard integration
    •    a config system
    •    per-language STT switching

User brings their own Codex.

⸻

9. Future improvements
    •    add VAD (Python side: use webrtcvad) so you don’t need fixed 5s recordings
    •    add /lang fr in the TUI to change STT language
    •    add “retry last transcript” without re-speaking
    •    add logging for latency (record → STT → Codex) so you can optimize

⸻

10. Minimal success criteria

You’re done when:
    1.    you can speak
    2.    a CLI makes text from it
    3.    that text goes to Codex
    4.    you see Codex output
    5.    you didn’t need OpenAI source code

That’s what we just designed.

⸻

Q1: Do you want me to rewrite the Python MVP to use ffmpeg instead of sounddevice so you have zero Python audio deps?
Q2: For the Rust TUI, do you prefer ratatui (more active) or are you okay with tui?
Q3: Are yo u planning to run STT with whisper or whisper.cpp (so I can shape the command format for you)? Does anything els neeed to be ad did to this plan does this architecture work? Thoughts on phases design choices etc


Yes. The architecture is sound. Add a few critical details and you’re set.

Gaps to close
	•	ffmpeg capture specifics: device per OS, mono 16 kHz for STT.
	•	whisper vs whisper.cpp flags: whisper uses model names (small), whisper.cpp needs a model file path.
	•	robust CLI plumbing: no shell=True, escape-free argv lists, timeouts, clear errors.
	•	latency logging: record durations for record → STT → Codex.
	•	graceful cleanup: temp files, SIGINT handling.
	•	config: optional TOML to avoid long flags.

Recommended defaults
	•	Capture: ffmpeg only. No Python audio deps.
	•	macOS: -f avfoundation -i ":0" -ac 1 -ar 16000
	•	Linux (Pulse): -f pulse -i default -ac 1 -ar 16000
	•	Windows: -f dshow -i audio="Default" (require --ffmpeg-device for reliability)
	•	STT:
	•	Start with whisper for simplicity.
	•	Switch to whisper.cpp for speed. Pass a model file like ggml-small.en.bin.
	•	TUI: use ratatui + crossterm + anyhow. Keep STT as an external process.

Python MVP (ffmpeg-based, one file)

Drop-in codex_voice.py. Implements your three functions, flaggable, with edit-before-send flow.

#!/usr/bin/env python3
import argparse, os, platform, shutil, subprocess, sys, tempfile, time, json
from pathlib import Path

def _require(cmd: str):
    if shutil.which(cmd) is None:
        raise RuntimeError(f"Command not found on PATH: {cmd}")

def _run(argv, *, input_bytes=None, timeout=None, cwd=None):
    p = subprocess.Popen(argv, stdin=subprocess.PIPE if input_bytes else None,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=cwd)
    try:
        out, err = p.communicate(input=input_bytes, timeout=timeout)
    except subprocess.TimeoutExpired:
        p.kill()
        out, err = p.communicate()
        raise RuntimeError(f"Timeout running: {' '.join(argv)}\n{err.decode(errors='ignore')}")
    if p.returncode != 0:
        raise RuntimeError(f"Nonzero exit {p.returncode}: {' '.join(argv)}\n{err.decode(errors='ignore')}")
    return out

def record_wav(path: str, seconds: int, ffmpeg_cmd: str, ffmpeg_device: str|None=None) -> None:
    _require(ffmpeg_cmd)
    sysname = platform.system()
    args = [ffmpeg_cmd, "-y"]
    if sysname == "Darwin":
        # list devices: ffmpeg -f avfoundation -list_devices true -i ""
        dev = ffmpeg_device if ffmpeg_device else ":0"
        args += ["-f", "avfoundation", "-i", dev]
    elif sysname == "Linux":
        # Try PulseAudio default. Users can pass --ffmpeg-device if needed.
        dev = ffmpeg_device if ffmpeg_device else "default"
        args += ["-f", "pulse", "-i", dev]
    elif sysname == "Windows":
        # Users should pass an exact device via --ffmpeg-device
        dev = ffmpeg_device if ffmpeg_device else "audio=Microphone (Default)"
        args += ["-f", "dshow", "-i", dev]
    else:
        raise RuntimeError(f"Unsupported OS: {sysname}")
    args += ["-t", str(seconds), "-ac", "1", "-ar", "16000", "-vn", path]
    _run(args)

def transcribe(path: str, whisper_cmd: str, lang: str, model: str, *, model_path: str|None=None, tmpdir: Path|None=None) -> str:
    _require(whisper_cmd)
    tmpdir = Path(tmpdir or tempfile.mkdtemp(prefix="codex_voice_"))
    base = tmpdir / "transcript"
    exe = Path(whisper_cmd).name.lower()

    if "whisper" == exe or exe.startswith("whisper"):
        # OpenAI whisper CLI
        # Writes <basename>.txt into output_dir
        out_dir = tmpdir
        args = [whisper_cmd, path, "--language", lang, "--model", model, "--output_format", "txt", "--output_dir", str(out_dir)]
        _run(args)
        txt_path = out_dir / (Path(path).stem + ".txt")
    else:
        # whisper.cpp style
        if not model_path:
            raise RuntimeError("whisper.cpp requires --whisper-model-path to a ggml*.bin file")
        args = [whisper_cmd, "-m", model_path, "-f", path, "-l", lang, "-otxt", "-of", str(base)]
        _run(args)
        txt_path = Path(str(base) + ".txt")

    if not txt_path.exists():
        raise RuntimeError(f"Transcript file not found: {txt_path}")
    return txt_path.read_text(encoding="utf-8").strip()

def call_codex_auto(prompt: str, codex_cmd: str, *, timeout: int|None=None) -> str:
    _require(codex_cmd)
    # First try argv mode: codex "<prompt>"
    try:
        out = _run([codex_cmd, prompt], timeout=timeout)
        return out.decode("utf-8", errors="ignore")
    except Exception:
        # Fallback: pipe prompt to stdin
        out = _run([codex_cmd], input_bytes=prompt.encode("utf-8"), timeout=timeout)
        return out.decode("utf-8", errors="ignore")

def main():
    ap = argparse.ArgumentParser(description="Voice → STT → Codex CLI")
    ap.add_argument("--seconds", type=int, default=5)
    ap.add_argument("--lang", default="en")
    ap.add_argument("--whisper-cmd", default="whisper", help="OpenAI whisper CLI or whisper.cpp binary")
    ap.add_argument("--whisper-model", default="small", help="name for whisper, ignored by whisper.cpp")
    ap.add_argument("--whisper-model-path", default=None, help="path to ggml*.bin for whisper.cpp")
    ap.add_argument("--codex-cmd", default="codex")
    ap.add_argument("--ffmpeg-cmd", default="ffmpeg")
    ap.add_argument("--ffmpeg-device", default=None, help="override input device string for ffmpeg")
    ap.add_argument("--say-ready", action="store_true", help="macOS say after Codex returns")
    ap.add_argument("--keep-audio", action="store_true")
    args = ap.parse_args()

    tmp = Path(tempfile.mkdtemp(prefix="codex_voice_"))
    wav = tmp / "audio.wav"

    t0 = time.monotonic()
    record_wav(str(wav), args.seconds, args.ffmpeg_cmd, args.ffmpeg_device)
    t1 = time.monotonic()
    transcript = transcribe(str(wav), args.whisper_cmd, args.lang, args.whisper_model,
                            model_path=args.whisper_model_path, tmpdir=tmp)
    t2 = time.monotonic()

    print("\n[Transcript]")
    print(transcript)
    print("\nPress Enter to send to Codex, or edit the text then Enter:")
    edited = input("> ").strip()
    prompt = edited if edited else transcript

    out = call_codex_auto(prompt, args.codex_cmd, timeout=180)
    t3 = time.monotonic()

    print("\n[Codex output]")
    print(out)

    metrics = {
        "record_s": round(t1 - t0, 3),
        "stt_s": round(t2 - t1, 3),
        "codex_s": round(t3 - t2, 3),
        "total_s": round(t3 - t0, 3),
    }
    print("\n[Latency]", json.dumps(metrics))

    if args.say_ready and platform.system() == "Darwin":
        try: _run(["say", "Codex result ready"])
        except Exception: pass

    if not args.keep_audio:
        try: wav.unlink(missing_ok=True)
        except Exception: pass

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

Usage examples
	•	macOS, OpenAI whisper:

python codex_voice.py --seconds 5 --whisper-cmd whisper --whisper-model small --lang en --codex-cmd codex

	•	macOS, whisper.cpp:

python codex_voice.py --seconds 5 --whisper-cmd ./main --whisper-model-path ./models/ggml-small.en.bin --lang en --codex-cmd codex

	•	Windows (example device):

python codex_voice.py --ffmpeg-device audio="Microphone (USB Audio Device)"

Rust TUI notes
	•	Keep STT and Codex calls behind a small trait:
	•	trait Runner { fn run(&self, argv: &[&str]) -> Result<String>; }
	•	Implement with std::process::Command.
	•	Use a bounded ring buffer for scrollback.
	•	Read child stdout incrementally with nonblocking pipes for streaming output.
	•	Persist last transcript and last prompt to a small history file in $XDG_STATE_HOME or ~/Library/Application Support.

Phase gates
	•	Phase 1: Python CLI works on macOS end to end. Latency printed. Manual edit flow in place.
	•	Phase 2: Rust TUI parity with Python UX. Nonblocking process I/O. /speak toggles STT. /lang xx updates STT lang.
	•	Phase 3: VAD option (webrtcvad in Python; same CLI interface in Rust). Config file and per-project profiles.

Answers
	•	Q1: Yes. Use ffmpeg. Simplifies setup and is portable. I already switched the MVP above.
	•	Q2: Use ratatui. It is more active and documented. Pair with crossterm and anyhow.
	•	Q3: Start with whisper for zero friction. Move to whisper.cpp for speed. Note the required model file path; your flag list needs --whisper-model-path for that case.

Add the --ffmpeg-device flag and --whisper-model-path. Everything else in your plan stands.

⸻

Phase 3 prep: VAD + config roadmap
	•	Introduce a shared config layer (TOML in $XDG_CONFIG_HOME/codex_voice/config.toml with per-flag overrides). Loader precedence: CLI flags > env vars > config file defaults.
	•	Extract the Python helpers into a module so Rust can call `python codex_voice.py --emit-json` until the Rust-native VAD lands.
	•	Add optional WebRTC VAD in Python first (segment the audio file, trim leading/trailing silence). Provide knobs: `--vad` flag, `--vad-padding-ms`, `--vad-threshold`.
	•	Port the VAD toggle to Rust by wrapping the Python helper initially; later swap to a Rust crate (e.g. `webrtc-vad` or `sonogram`) behind the same interface.
	•	Update the Rust TUI state machine: `/vad on|off`, surface the active language/profile in the status bar, keep the transcript history per profile on disk.
	•	Write smoke tests for the config loader and VAD toggle (Python unit test + Rust integration test using stub audio like the current fake_ffmpeg).
