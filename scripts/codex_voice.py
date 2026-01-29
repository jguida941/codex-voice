#!/usr/bin/env python3
"""Voice → Whisper → Codex pipeline with reusable building blocks.

The module intentionally keeps the workflow in three portable stages:

1. Capture microphone audio with `record_wav`, which shells out to `ffmpeg`
   using OS-specific defaults so other languages can mirror the behaviour.
2. Transcribe the captured clip with `transcribe`, delegating to either the
   OpenAI `whisper` CLI or the `whisper.cpp` binary.
3. Forward the final prompt to Codex via `call_codex_auto`, automatically
   retrying argv/stdin/PTY modes when the CLI insists on a TTY.

Everything is wrapped in small dataclasses (`PipelineConfig`, `PipelineResult`)
so frontends such as the Rust TUI or automated tests can run the pipeline
non-interactively (`--auto-send --emit-json`) and treat this script as the
canonical spec.
"""
import argparse, errno, json, os, platform, pty, select, shlex, shutil, subprocess, sys, tempfile, time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Optional

# Extra Codex CLI flags injected via --codex-args/--codex-arg; stored globally for reuse.
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
    proc = None
    try:
        proc = subprocess.Popen(argv, stdin=slave_fd, stdout=slave_fd, stderr=slave_fd, env=env)
    except Exception:
        os.close(master_fd)
        os.close(slave_fd)
        raise
    finally:
        # Child inherits the slave; close our parent copy.
        if proc is not None:
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

def transcribe(path: str, whisper_cmd: str, lang: str, model: str, *, model_path: str|None=None, tmpdir: Path|None=None) -> tuple[str, Path]:
    """Convert recorded audio into text using the selected Whisper implementation.

    This helper accepts both the official OpenAI CLI (`whisper`) and the
    whisper.cpp binary, mirroring the flags required by each tool. Temporary
    files are written into a per-invocation directory so that multiple runs
    never collide.
    Returns:
        A tuple of the transcript text and the path to the generated `.txt` file.
    """
    _require(whisper_cmd)
    tmpdir = Path(tmpdir or tempfile.mkdtemp(prefix="codex_voice_"))
    base = tmpdir / "transcript"
    exe = Path(whisper_cmd).name.lower()

    lang = lang.strip().lower()
    use_auto = lang == "auto"

    if "whisper" == exe or exe.startswith("whisper"):
        # OpenAI whisper CLI
        # Writes <basename>.txt into output_dir
        out_dir = tmpdir
        args = [whisper_cmd, path, "--model", model, "--output_format", "txt", "--output_dir", str(out_dir)]
        if not use_auto:
            args += ["--language", lang]
        _run(args)
        txt_path = out_dir / (Path(path).stem + ".txt")
    else:
        # whisper.cpp style
        if not model_path:
            raise RuntimeError("whisper.cpp requires --whisper-model-path to a ggml*.bin file")
        args = [whisper_cmd, "-m", model_path, "-f", path, "-otxt", "-of", str(base)]
        if use_auto:
            args += ["-l", "auto"]
        else:
            args += ["-l", lang]
        _run(args)
        txt_path = Path(str(base) + ".txt")

    if not txt_path.exists():
        raise RuntimeError(f"Transcript file not found: {txt_path}")
    return txt_path.read_text(encoding="utf-8").strip(), txt_path

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


@dataclass
class PipelineConfig:
    """Immutable configuration for a single voice → text → Codex run."""

    seconds: int = 5
    lang: str = "en"
    whisper_cmd: str = "whisper"
    whisper_model: str = "small"
    whisper_model_path: str | None = None
    codex_cmd: str = "codex"
    ffmpeg_cmd: str = "ffmpeg"
    ffmpeg_device: str | None = None
    codex_timeout: int | None = 180
    keep_audio: bool = False
    run_codex: bool = True


@dataclass
class CaptureArtifacts:
    """Intermediate data produced after recording+transcribing a single clip."""

    transcript: str
    wav_path: Path
    transcript_path: Path | None
    metrics: dict[str, float]
    tmp_dir: Path
    artifacts_retained: bool
    _cleanup: Optional[Callable[[], None]] = None

    def cleanup(self) -> None:
        """Delete temporary artifacts when they were not requested to persist."""
        if self._cleanup:
            try:
                self._cleanup()
            finally:
                self._cleanup = None


@dataclass
class PipelineResult:
    """Final outcome of a full capture/transcription/Codex run."""

    transcript: str
    prompt: str
    codex_output: str | None
    metrics: dict[str, float]
    audio_path: str | None
    transcript_path: str | None
    artifacts_retained: bool

    def to_dict(self) -> dict[str, object]:
        """Return a JSON-safe dictionary describing the run."""
        return {
            "transcript": self.transcript,
            "prompt": self.prompt,
            "codex_output": self.codex_output,
            "metrics": self.metrics,
            "artifacts_retained": self.artifacts_retained,
            "paths": {
                "audio": self.audio_path,
                "transcript": self.transcript_path,
            },
        }


def _prepare_tmp_dir(keep_audio: bool) -> tuple[Path, bool, Callable[[], None]]:
    """Return a temporary directory, a retention flag, and a cleanup callback."""
    if keep_audio:
        path = Path(tempfile.mkdtemp(prefix="codex_voice_"))
        return path, True, lambda: None
    tmp = tempfile.TemporaryDirectory(prefix="codex_voice_")
    path = Path(tmp.name)

    def _cleanup():
        tmp.cleanup()

    return path, False, _cleanup


def capture_transcript(config: PipelineConfig) -> CaptureArtifacts:
    """Record and transcribe audio according to `config`, returning artifacts."""
    tmp_dir, retained, cleanup_cb = _prepare_tmp_dir(config.keep_audio)
    try:
        wav = tmp_dir / "audio.wav"
        t0 = time.monotonic()
        record_wav(str(wav), config.seconds, config.ffmpeg_cmd, config.ffmpeg_device)
        t1 = time.monotonic()
        transcript_text, transcript_path = transcribe(
            str(wav),
            config.whisper_cmd,
            config.lang,
            config.whisper_model,
            model_path=config.whisper_model_path,
            tmpdir=tmp_dir,
        )
        t2 = time.monotonic()
        metrics = {
            "record_s": round(t1 - t0, 3),
            "stt_s": round(t2 - t1, 3),
        }
        return CaptureArtifacts(
            transcript=transcript_text,
            wav_path=wav,
            transcript_path=transcript_path,
            metrics=metrics,
            tmp_dir=tmp_dir,
            artifacts_retained=retained,
            _cleanup=cleanup_cb,
        )
    except Exception:
        cleanup_cb()
        raise


def finalize_pipeline(artifacts: CaptureArtifacts, config: PipelineConfig, prompt_override: str | None = None) -> PipelineResult:
    """Send the chosen prompt to Codex (when enabled) and build a result."""
    prompt = prompt_override if prompt_override is not None else artifacts.transcript
    metrics = dict(artifacts.metrics)
    codex_out: str | None = None
    codex_duration = 0.0
    if config.run_codex and prompt:
        codex_start = time.monotonic()
        codex_out = call_codex_auto(prompt, config.codex_cmd, timeout=config.codex_timeout)
        codex_duration = time.monotonic() - codex_start
    metrics["codex_s"] = round(codex_duration, 3)
    metrics["total_s"] = round(metrics["record_s"] + metrics["stt_s"] + metrics["codex_s"], 3)
    audio_path = str(artifacts.wav_path) if artifacts.artifacts_retained else None
    transcript_path = str(artifacts.transcript_path) if artifacts.artifacts_retained and artifacts.transcript_path else None
    return PipelineResult(
        transcript=artifacts.transcript,
        prompt=prompt,
        codex_output=codex_out,
        metrics=metrics,
        audio_path=audio_path,
        transcript_path=transcript_path,
        artifacts_retained=artifacts.artifacts_retained,
    )


def run_pipeline(config: PipelineConfig, prompt_override: str | None = None) -> PipelineResult:
    """Capture → transcribe → (optional) Codex in one shot, cleaning up safely."""
    artifacts = capture_transcript(config)
    try:
        return finalize_pipeline(artifacts, config, prompt_override=prompt_override)
    finally:
        artifacts.cleanup()

def main():
    """High-level CLI entrypoint for the voice → Whisper → Codex workflow.

    The routine wires together temp directory management, interactive editing of
    the transcript, reporting of latency metrics, and optional macOS voice
    feedback. It also handles polite cleanup of temporary audio artifacts.
    """
    ap = argparse.ArgumentParser(description="Voice → STT → Codex CLI", allow_abbrev=False)
    ap.add_argument("--seconds", type=int, default=5)
    ap.add_argument("--lang", default="en", help="Whisper language (ISO-639-1 or 'auto')")
    ap.add_argument("--whisper-cmd", default="whisper", help="OpenAI whisper CLI or whisper.cpp binary")
    ap.add_argument("--whisper-model", default="small", help="name for whisper, ignored by whisper.cpp")
    ap.add_argument("--whisper-model-path", default=None, help="path to ggml*.bin for whisper.cpp")
    ap.add_argument("--codex-cmd", default="codex")
    ap.add_argument("--ffmpeg-cmd", default="ffmpeg")
    ap.add_argument("--ffmpeg-device", default=None, help="override input device string for ffmpeg")
    ap.add_argument("--codex-args", default="", help="extra arguments appended when invoking Codex")
    ap.add_argument(
        "--codex-arg",
        action="append",
        default=[],
        help="repeatable Codex argument (avoids shell quoting issues)",
    )
    ap.add_argument("--say-ready", action="store_true", help="macOS say after Codex returns")
    ap.add_argument("--keep-audio", action="store_true", help="retain the temp directory with audio/transcript artifacts")
    ap.add_argument("--auto-send", action="store_true", help="skip the edit prompt and immediately send the transcript to Codex")
    ap.add_argument("--emit-json", action="store_true", help="print a machine-readable JSON summary (suppresses interactive prompts)")
    ap.add_argument("--no-codex", action="store_true", help="stop after transcription instead of calling Codex")
    ap.add_argument("--codex-timeout", type=int, default=180, help="timeout (seconds) for Codex invocations")
    args = ap.parse_args()

    global _EXTRA_CODEX_ARGS
    # Persist additional Codex flags so helper functions can reuse them.
    _EXTRA_CODEX_ARGS = []
    if getattr(args, "codex_args", None):
        _EXTRA_CODEX_ARGS.extend(shlex.split(args.codex_args))
    if getattr(args, "codex_arg", None):
        _EXTRA_CODEX_ARGS.extend(args.codex_arg)

    config = PipelineConfig(
        seconds=args.seconds,
        lang=args.lang,
        whisper_cmd=args.whisper_cmd,
        whisper_model=args.whisper_model,
        whisper_model_path=args.whisper_model_path,
        codex_cmd=args.codex_cmd,
        ffmpeg_cmd=args.ffmpeg_cmd,
        ffmpeg_device=args.ffmpeg_device,
        codex_timeout=args.codex_timeout,
        keep_audio=args.keep_audio,
        run_codex=not args.no_codex,
    )

    if args.emit_json or args.auto_send or args.no_codex:
        # Non-interactive mode: run everything (or stop after STT) automatically.
        result = run_pipeline(config)
        if not args.emit_json:
            _print_human_summary(result)
        else:
            print(json.dumps(result.to_dict(), ensure_ascii=False))
        return

    # Interactive flow: capture once, allow manual edits, then send.
    artifacts = capture_transcript(config)
    try:
        print("\n[Transcript]")
        print(artifacts.transcript)
        prompt = artifacts.transcript
        if config.run_codex:
            print("\nPress Enter to send to Codex, or edit the text then Enter:")
            edited = input("> ").strip()
            prompt = edited if edited else artifacts.transcript

        if config.run_codex:
            print("\n[Codex output]")
            sys.stdout.flush()

        result = finalize_pipeline(artifacts, config, prompt_override=prompt)
        if config.run_codex and result.codex_output is not None:
            print(result.codex_output)
        _print_human_summary(result, repeat_transcript=False, include_buffer=False)
    finally:
        artifacts.cleanup()

    if args.say_ready and platform.system() == "Darwin":
        # Offer audible feedback when the command completes on macOS.
        try:
            _run(["say", "Codex result ready"])
        except Exception:
            pass


def _print_human_summary(result: PipelineResult, *, repeat_transcript: bool = True, include_buffer: bool = True) -> None:
    """Render transcript, Codex output, and latency metrics for humans."""
    if repeat_transcript:
        print("\n[Transcript]")
        print(result.transcript)

    if include_buffer and result.codex_output is not None:
        print("\n[Codex output]")
        print(result.codex_output)

    print("\n[Latency]", json.dumps(result.metrics))

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
