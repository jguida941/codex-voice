"""PyPI launcher for bootstrapping and running the native VoiceTerm binary."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


DEFAULT_REPO_URL = "https://github.com/jguida941/voiceterm"


def _native_root() -> Path:
    configured = os.environ.get("VOICETERM_PY_NATIVE_ROOT")
    if configured:
        return Path(configured).expanduser()
    return Path.home() / ".local" / "share" / "voiceterm" / "native"


def _native_bin() -> Path:
    configured = os.environ.get("VOICETERM_NATIVE_BIN")
    if configured:
        return Path(configured).expanduser()
    return _native_root() / "bin" / "voiceterm"


def _run(cmd: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, check=check, text=True)


def _bootstrap_native_bin() -> Path:
    cargo = shutil.which("cargo")
    git = shutil.which("git")
    if not cargo or not git:
        missing = []
        if not git:
            missing.append("git")
        if not cargo:
            missing.append("cargo")
        raise RuntimeError(
            "Missing required bootstrap tools: "
            + ", ".join(missing)
            + ". Install them or set VOICETERM_NATIVE_BIN."
        )

    root = _native_root()
    root.mkdir(parents=True, exist_ok=True)
    repo_url = os.environ.get("VOICETERM_REPO_URL", DEFAULT_REPO_URL)

    with tempfile.TemporaryDirectory(prefix="voiceterm-bootstrap-") as tmp:
        repo_dir = Path(tmp) / "repo"
        _run([git, "clone", "--depth", "1", repo_url, str(repo_dir)])
        manifest_dir = repo_dir / "src"
        if not manifest_dir.exists():
            raise RuntimeError(
                f"Expected Cargo project at {manifest_dir}, but it does not exist."
            )
        _run(
            [
                cargo,
                "install",
                "--locked",
                "--root",
                str(root),
                "--path",
                str(manifest_dir),
                "--bin",
                "voiceterm",
            ]
        )

    native = _native_bin()
    if not native.exists():
        raise RuntimeError(f"Bootstrap completed but binary was not found at {native}.")
    return native


def _ensure_native_bin() -> Path:
    native = _native_bin()
    if native.exists():
        return native
    return _bootstrap_native_bin()


def main() -> int:
    try:
        native = _ensure_native_bin()
    except Exception as err:  # pragma: no cover - user-facing launcher error
        print(f"voiceterm launcher error: {err}", file=sys.stderr)
        print(
            "Install native VoiceTerm manually or set VOICETERM_NATIVE_BIN.",
            file=sys.stderr,
        )
        return 1

    try:
        completed = subprocess.run([str(native), *sys.argv[1:]])
        return int(completed.returncode)
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    raise SystemExit(main())

