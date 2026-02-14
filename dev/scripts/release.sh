#!/usr/bin/env bash
#
# VoiceTerm Release Script
# Usage: ./dev/scripts/release.sh <version>
# Example: ./dev/scripts/release.sh 1.0.33
#
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.0.33"
    exit 1
fi

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g., 1.0.33)"
    exit 1
fi

TAG="v$VERSION"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CARGO_TOML="$REPO_ROOT/src/Cargo.toml"
CHANGELOG="$REPO_ROOT/dev/CHANGELOG.md"

echo "=== VoiceTerm Release $TAG ==="

# Safety checks: enforce branch + clean working tree + version alignment.
# Check we're on master
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$BRANCH" != "master" ]]; then
    echo "Error: Must be on master branch (currently on $BRANCH)"
    exit 1
fi

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo "Error: Uncommitted changes detected. Commit or stash them first."
    exit 1
fi

# Check Cargo.toml version matches
CARGO_VERSION=$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')
if [[ "$CARGO_VERSION" != "$VERSION" ]]; then
    echo "Error: Cargo.toml version ($CARGO_VERSION) doesn't match release version ($VERSION)"
    echo "Update src/Cargo.toml first."
    exit 1
fi

# Check CHANGELOG has entry for this version
if ! grep -q "## \[$VERSION\]" "$CHANGELOG" && ! grep -q "## $VERSION" "$CHANGELOG"; then
    echo "Warning: No CHANGELOG entry found for version $VERSION"
    read -p "Continue anyway? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Pull latest
echo "Pulling latest changes..."
git pull origin master

# Create tag
echo "Creating tag $TAG..."
git tag -a "$TAG" -m "Release $TAG"

# Push tag
echo "Pushing tag to origin..."
git push origin "$TAG"

echo ""
echo "=== Tag $TAG pushed ==="
echo ""
echo "Next steps:"
echo "1. Create GitHub release: gh release create $TAG --title '$TAG' --notes 'See CHANGELOG.md'"
echo "2. Run: ./dev/scripts/publish-pypi.sh --upload"
echo "3. Run: ./dev/scripts/update-homebrew.sh $VERSION"
echo ""
