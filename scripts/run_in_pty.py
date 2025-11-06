#!/usr/bin/env python3
"""
Drop-in helper that runs a command inside a pseudo-terminal.

Used by the Rust TUI when the Codex CLI refuses to execute without a TTY. It
mirrors the Python `_run_with_pty` helper already present in codex_voice.py so
both frontends can share the same behaviour.
"""

from __future__ import annotations

import argparse
import errno
import os
import pty
import select
import sys
import time


def _drain(master_fd: int) -> bytes:
    """Read all available bytes from the PTY master."""
    buf = bytearray()
    cursor_reply = b"\x1b[1;1R"

    while True:
        r, _, _ = select.select([master_fd], [], [], 0.1)
        if master_fd not in r:
            break
        try:
            chunk = os.read(master_fd, 1024)
        except OSError as exc:  # pragma: no cover
            if exc.errno == errno.EIO:
                break
            raise
        if not chunk:
            break
        # Some CLIs probe cursor position; respond so they do not block forever.
        if b"\x1b[6n" in chunk:
            chunk = chunk.replace(b"\x1b[6n", b"")
            os.write(master_fd, cursor_reply)
        if chunk:
            buf.extend(chunk)
    return bytes(buf)


def run_with_pty(argv: list[str], stdin_data: bytes | None) -> int:
    master_fd, slave_fd = pty.openpty()
    try:
        pid = os.fork()
    except Exception:
        os.close(master_fd)
        os.close(slave_fd)
        raise

    if pid == 0:  # Child
        try:
            os.close(master_fd)
            os.environ.setdefault("TERM", os.environ.get("TERM", "xterm-256color"))
            os.execvp(argv[0], argv)
        finally:  # pragma: no cover - defensive only
            os._exit(1)

    # Parent process
    os.close(slave_fd)

    if stdin_data:
        os.write(master_fd, stdin_data)

    output = bytearray()
    exit_code: int | None = None
    start = time.monotonic()

    while exit_code is None:
        output.extend(_drain(master_fd))
        pid_status = os.waitpid(pid, os.WNOHANG)
        if pid_status[0] == pid:
            if os.WIFEXITED(pid_status[1]):
                exit_code = os.WEXITSTATUS(pid_status[1])
            else:
                exit_code = 1
        else:
            if time.monotonic() - start > 300:  # pragma: no cover
                exit_code = 1
                os.kill(pid, 9)

    # Drain any trailing output
    output.extend(_drain(master_fd))
    os.close(master_fd)

    sys.stdout.buffer.write(output)
    return exit_code


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Run a command inside a PTY.")
    parser.add_argument("--stdin", action="store_true", help="pipe stdin into the child process")
    parser.add_argument("cmd", nargs=argparse.REMAINDER, help="command to execute")
    args = parser.parse_args(argv)

    if not args.cmd:
        parser.error("missing command to execute")

    payload = None
    if args.stdin:
        payload = sys.stdin.buffer.read()
        if payload and not payload.endswith(b"\n"):
            payload += b"\n"

    return run_with_pty(args.cmd, payload)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
