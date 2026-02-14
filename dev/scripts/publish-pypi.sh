#!/usr/bin/env bash
#
# Build and optionally publish the VoiceTerm PyPI package.
# Usage:
#   ./dev/scripts/publish-pypi.sh
#   ./dev/scripts/publish-pypi.sh --upload
#
set -euo pipefail

UPLOAD=0
if [[ "${1:-}" == "--upload" ]]; then
    UPLOAD=1
fi

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PYPI_DIR="$REPO_ROOT/pypi"
DIST_DIR="$PYPI_DIR/dist"
CARGO_TOML="$REPO_ROOT/src/Cargo.toml"
PYPROJECT="$PYPI_DIR/pyproject.toml"

if [[ ! -f "$PYPROJECT" ]]; then
    echo "Error: missing $PYPROJECT"
    exit 1
fi

if [[ ! -f "$CARGO_TOML" ]]; then
    echo "Error: missing $CARGO_TOML"
    exit 1
fi

CARGO_VERSION="$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')"
PYPI_VERSION="$(grep '^version = ' "$PYPROJECT" | head -1 | sed 's/version = "\(.*\)"/\1/')"

if [[ "$CARGO_VERSION" != "$PYPI_VERSION" ]]; then
    echo "Error: version mismatch"
    echo "  src/Cargo.toml: $CARGO_VERSION"
    echo "  pypi/pyproject.toml: $PYPI_VERSION"
    exit 1
fi

echo "Building PyPI package for VoiceTerm v$PYPI_VERSION"
cd "$PYPI_DIR"
rm -rf "$DIST_DIR"

python3 -m build
python3 -m twine check dist/*

if [[ $UPLOAD -eq 1 ]]; then
    echo "Uploading to PyPI..."
    python3 -m twine upload dist/*
    echo "Upload complete."
else
    echo "Build complete. Artifacts:"
    ls -lh dist
    echo ""
    echo "To publish: ./dev/scripts/publish-pypi.sh --upload"
fi

