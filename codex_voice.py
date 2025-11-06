#!/usr/bin/env python3
"""Record microphone audio, transcribe it, and forward the text to the Codex CLI.

This module intentionally keeps the pipeline in three reusable steps:

1. Capture audio with `record_wav`, which shells out to ffmpeg using
   platform-specific defaults so that other languages can mirror the behavior.
2. Convert the audio into text via `transcribe`, delegating to either the
   OpenAI `whisper` CLI or the whisper.cpp binary.
3. Send the prompt to Codex with `call_codex_auto`, automatically retrying with
   stdin or an emulated TTY when the CLI rejects non-interactive runs.

The Rust TUI in `rust_tui/` reuses these same shell contracts, so keeping this
module well documented makes it easier to keep both frontends aligned.
"""
import argparse, errno, json, os, platform, pty, select, shlex, shutil, subprocess, sys, tempfile, time
from pathlib import Path

# Extra Codex CLI flags injected via --codex-args; stored globally for reuse.
_EXTRA_CODEX_ARGS: list[str] = []

def _require(cmd: str):
    """Ensure a command is present on the PATH before dispatching a subprocess.

    Raises:
        RuntimeError: if the command cannot be found with `shutil.which`.
    """
    if shutil.which(cmd) is None:
        raise RuntimeError(f"Command not found on PATH: {cmd}")

def _run(argv, *, input_bytes=None, timeout=None, cwd=None, env=None):
    """Execute a command and return its stdout bytes.

    Args:
        argv: Sequence passed to `subprocess.Popen`.
        input_bytes: Optional stdin payload supplied once, with a newline added
            by the caller when required.
        timeout: Optional ceiling in seconds before the subprocess is killed.
        cwd: Optional working directory override.
        env: Optional environment block for the child process.

    Raises:
        RuntimeError: if the command times out or exits non-zero. The error
        includes stderr output so failures are easier to diagnose.
    """
    p = subprocess.Popen(argv, stdin=subprocess.PIPE if input_bytes else None,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=cwd, env=env)
    try:
        out, err = p.communicate(input=input_bytes, timeout=timeout)
    except subprocess.TimeoutExpired:
        p.kill()
        out, err = p.communicate()
        raise RuntimeError(f"Timeout running: {' '.join(argv)}\n{err.decode(errors='ignore')}")
    if p.returncode != 0:
        raise RuntimeError(f"Nonzero exit {p.returncode}: {' '.join(argv)}\n{err.decode(errors='ignore')}")
    return out

def _run_with_pty(argv, *, input_bytes=None, timeout=None, env=None):
    """Run a command within a pseudo-terminal and capture its output.

    Some Codex CLI flows emit a "stdout is not a TTY" error when started from a
    non-interactive pipe. In those situations we fall back to a PTY so the CLI
    believes it is talking to a terminal.
    """
    if platform.system() == "Windows":
        raise RuntimeError("PTY fallback is not supported on Windows")

    master_fd, slave_fd = pty.openpty()
    cursor_report = b"\x1b[1;1R"
    try:
        proc = subprocess.Popen(argv, stdin=slave_fd, stdout=slave_fd, stderr=slave_fd, env=env)
    except Exception:
        os.close(master_fd)
        os.close(slave_fd)
        raise
    finally:
        # Child inherits the slave; close our parent copy.
        os.close(slave_fd)

    if input_bytes:
        data = input_bytes
        if not data.endswith(b"\n"):
            data += b"\n"
        os.write(master_fd, data)

    out = bytearray()
    start = time.monotonic()

    def _read_chunk():
        try:
            return os.read(master_fd, 1024)
        except OSError as e:
            if e.errno == errno.EIO:
                return b""
            raise

    try:
        while True:
            if timeout is not None:
                elapsed = time.monotonic() - start
                remaining = timeout - elapsed
                if remaining <= 0:
                    proc.kill()
                    proc.wait()
                    raise RuntimeError(f"Timeout running (PTY): {' '.join(argv)}")
                wait = max(0.0, min(0.1, remaining))
            else:
                wait = 0.1

            r, _, _ = select.select([master_fd], [], [], wait)
            if master_fd in r:
                chunk = _read_chunk()
                if chunk:
                    if b"\x1b[6n" in chunk:
                        chunk = chunk.replace(b"\x1b[6n", b"")
                        os.write(master_fd, cursor_report)
                    if chunk:
                        out.extend(chunk)
                elif proc.poll() is not None:
                    break
            if proc.poll() is not None:
                while True:
                    chunk = _read_chunk()
                    if not chunk:
                        break
                    if b"\x1b[6n" in chunk:
                        chunk = chunk.replace(b"\x1b[6n", b"")
                        os.write(master_fd, cursor_report)
                    if chunk:
                        out.extend(chunk)
                break
    finally:
        os.close(master_fd)

    if proc.returncode != 0:
        raise RuntimeError(f"Nonzero exit {proc.returncode} (PTY): {' '.join(argv)}\n{out.decode('utf-8', errors='ignore')}")
    return bytes(out)

def _is_tty_error(error: Exception) -> bool:
    """Return True when the exception text suggests a missing TTY."""
    msg = str(error).lower()
    return "stdout is not a terminal" in msg or "isatty" in msg or "not a tty" in msg

def record_wav(path: str, seconds: int, ffmpeg_cmd: str, ffmpeg_device: str|None=None) -> None:
    """Capture microphone input to a mono, 16 kHz WAV file via ffmpeg.

    The function chooses reasonable defaults for each operating system so the
    caller rarely needs to know the exact device names. When defaults do not
    work the optional `ffmpeg_device` argument allows full override.
    """
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
    """Convert recorded audio into text using the selected Whisper implementation.

    This helper accepts both the official OpenAI CLI (`whisper`) and the
    whisper.cpp binary, mirroring the flags required by each tool. Temporary
    files are written into a per-invocation directory so that multiple runs
    never collide.
    """
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

def call_codex_auto(prompt: str, codex_cmd: str, *, timeout: int | None = None) -> str | None:
    """Invoke the Codex CLI and gracefully fallback across invocation modes.

    The function first tries to run Codex in "argument mode" (passing the prompt
    as a positional argument) and, if that fails, switches to piping the prompt
    via stdin. When Codex refuses to run without a TTY we emulate one using a
    pseudo-terminal so the same behavior works inside scripts and tests. Any
    extra Codex flags supplied via `--codex-args` are threaded through every
    attempt.

    Returns:
        Either the captured stdout text (when running in a non-interactive
        environment) or None if Codex wrote directly to the parent TTY.
    """
    _require(codex_cmd)
    prompt_bytes = prompt.encode("utf-8")
    error_messages: list[str] = []

    # Allow higher-level wrappers (like the Rust TUI) to inject extra Codex CLI flags.
    extra_args = list(_EXTRA_CODEX_ARGS)
    env = os.environ.copy()
    env.setdefault("TERM", env.get("TERM", "xterm-256color"))

    if sys.stdout.isatty():
        # Fast path: when the parent is an interactive shell prefer streaming output
        # directly so Codex can render progress/UI elements untouched.
        cmd1 = [codex_cmd, *extra_args, prompt]
        result = subprocess.run(
            cmd1,
            check=False,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )
        if result.returncode == 0:
            return None
        error_messages.append(
            f"Arg mode exit {result.returncode}: {' '.join(cmd1)}\n{(result.stderr or '').strip()}"
        )

        input_text = prompt if prompt.endswith("\n") else prompt + "\n"
        cmd2 = [codex_cmd, *extra_args]
        result = subprocess.run(
            cmd2,
            input=input_text,
            check=False,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )
        if result.returncode == 0:
            return None
        error_messages.append(
            f"Stdin mode exit {result.returncode}: {' '.join(cmd2)}\n{(result.stderr or '').strip()}"
        )

    attempts = [
        ([codex_cmd, *extra_args, prompt], {}),
        ([codex_cmd, *extra_args], {"input_bytes": prompt_bytes}),
    ]

    for argv, extra in attempts:
        try:
            out = _run(argv, timeout=timeout, env=env, **extra)
            return out.decode("utf-8", errors="ignore")
        except RuntimeError as exc:
            error_messages.append(str(exc))
            if _is_tty_error(exc) and platform.system() != "Windows":
                try:
                    out = _run_with_pty(argv, timeout=timeout, env=env, **extra)
                    return out.decode("utf-8", errors="ignore")
                except Exception as pty_exc:
                    error_messages.append(f"PTY fallback failed: {pty_exc}")

    joined = "\n---\n".join(error_messages)
    raise RuntimeError(f"Codex invocation failed:\n{joined}")

def main():
    """High-level CLI entrypoint for the voice → Whisper → Codex workflow.

    The routine wires together temp directory management, interactive editing of
    the transcript, reporting of latency metrics, and optional macOS voice
    feedback. It also handles polite cleanup of temporary audio artifacts.
    """
    ap = argparse.ArgumentParser(description="Voice → STT → Codex CLI")
    ap.add_argument("--seconds", type=int, default=5)
    ap.add_argument("--lang", default="en")
    ap.add_argument("--whisper-cmd", default="whisper", help="OpenAI whisper CLI or whisper.cpp binary")
    ap.add_argument("--whisper-model", default="small", help="name for whisper, ignored by whisper.cpp")
    ap.add_argument("--whisper-model-path", default=None, help="path to ggml*.bin for whisper.cpp")
    ap.add_argument("--codex-cmd", default="codex")
    ap.add_argument("--ffmpeg-cmd", default="ffmpeg")
    ap.add_argument("--ffmpeg-device", default=None, help="override input device string for ffmpeg")
    ap.add_argument("--codex-args", default="", help="extra arguments appended when invoking Codex")
    ap.add_argument("--say-ready", action="store_true", help="macOS say after Codex returns")
    ap.add_argument("--keep-audio", action="store_true")
    args = ap.parse_args()

    global _EXTRA_CODEX_ARGS
    # Persist additional Codex flags so helper functions can reuse them.
    _EXTRA_CODEX_ARGS = shlex.split(args.codex_args) if getattr(args, "codex_args", None) else []

    # Use a private temp directory for the short-lived audio file and transcripts.
    tmp = Path(tempfile.mkdtemp(prefix="codex_voice_"))
    wav = tmp / "audio.wav"

    t0 = time.monotonic()
    # Record a single-channel clip that downstream whisper tools can consume directly.
    record_wav(str(wav), args.seconds, args.ffmpeg_cmd, args.ffmpeg_device)
    t1 = time.monotonic()
    # Convert the captured audio into text using the selected Whisper implementation.
    transcript = transcribe(str(wav), args.whisper_cmd, args.lang, args.whisper_model,
                            model_path=args.whisper_model_path, tmpdir=tmp)
    t2 = time.monotonic()

    print("\n[Transcript]")
    print(transcript)
    print("\nPress Enter to send to Codex, or edit the text then Enter:")
    edited = input("> ").strip()
    prompt = edited if edited else transcript

    # Forward the final prompt to the Codex CLI and capture its response.
    print("\n[Codex output]")
    sys.stdout.flush()  # Ensure headers are on screen before Codex starts talking.
    out = call_codex_auto(prompt, args.codex_cmd, timeout=180)
    t3 = time.monotonic()
    if out is not None:
        # Non-interactive runs buffer Codex output; emit it ourselves.
        print(out)

    # Track end-to-end timings so users can gauge latency across the pipeline.
    metrics = {
        "record_s": round(t1 - t0, 3),
        "stt_s": round(t2 - t1, 3),
        "codex_s": round(t3 - t2, 3),
        "total_s": round(t3 - t0, 3),
    }
    print("\n[Latency]", json.dumps(metrics))

    if args.say_ready and platform.system() == "Darwin":
        # Offer audible feedback when the command completes on macOS.
        try:
            _run(["say", "Codex result ready"])
        except Exception:
            pass

    if not args.keep_audio:
        # Clean up the temporary audio clip unless the caller asked to keep it.
        try:
            wav.unlink(missing_ok=True)
        except Exception:
            pass

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
