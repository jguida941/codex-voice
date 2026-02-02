#!/usr/bin/env bash
#
# Update Homebrew tap for VoxTerm
# Usage: ./dev/scripts/update-homebrew.sh <version>
# Example: ./dev/scripts/update-homebrew.sh 1.0.33
#
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.0.33"
    exit 1
fi

TAG="v$VERSION"

resolve_homebrew_repo() {
    if [[ -n "${HOMEBREW_VOXTERM_PATH:-}" ]]; then
        echo "$HOMEBREW_VOXTERM_PATH"
        return 0
    fi

    if command -v brew >/dev/null 2>&1; then
        local repo
        repo="$(brew --repo jguida941/voxterm 2>/dev/null || true)"
        if [[ -n "$repo" && -d "$repo" ]]; then
            echo "$repo"
            return 0
        fi

        repo="$(brew --repo jguida941/homebrew-voxterm 2>/dev/null || true)"
        if [[ -n "$repo" && -d "$repo" ]]; then
            echo "$repo"
            return 0
        fi
    fi

    echo "$HOME/testing_upgrade/homebrew-voxterm"
}

HOMEBREW_REPO="$(resolve_homebrew_repo)"
FORMULA="$HOMEBREW_REPO/Formula/voxterm.rb"
README="$HOMEBREW_REPO/README.md"

echo "=== Updating Homebrew tap for $TAG ==="

# Check Homebrew repo exists
if [[ ! -d "$HOMEBREW_REPO" ]]; then
    echo "Error: Homebrew repo not found at $HOMEBREW_REPO"
    echo "Set HOMEBREW_VOXTERM_PATH or clone the repo first."
    exit 1
fi

# Get SHA256 of release tarball
TARBALL_URL="https://github.com/jguida941/voxterm/archive/refs/tags/$TAG.tar.gz"
echo "Fetching SHA256 for $TARBALL_URL..."
SHA256=$(curl -sL "$TARBALL_URL" | shasum -a 256 | cut -d' ' -f1)

if [[ -z "$SHA256" || "$SHA256" == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" ]]; then
    echo "Error: Failed to get SHA256 (empty tarball or tag doesn't exist)"
    echo "Make sure tag $TAG exists on GitHub."
    exit 1
fi

echo "SHA256: $SHA256"

# Update formula
echo "Updating $FORMULA..."
cd "$HOMEBREW_REPO"

# Update version
sed -i '' "s|url \"https://github.com/jguida941/voxterm/archive/refs/tags/v[0-9.]*\.tar\.gz\"|url \"$TARBALL_URL\"|" "$FORMULA"
sed -i '' "s|version \"[0-9.]*\"|version \"$VERSION\"|" "$FORMULA"
sed -i '' "s|sha256 \"[a-f0-9]*\"|sha256 \"$SHA256\"|" "$FORMULA"

# Update README version + model path if present
if [[ -f "$README" ]]; then
    sed -i '' "s/^Current: v[0-9.]*$/Current: v$VERSION/" "$README"
    sed -i '' "s|ls \\$(brew --prefix)/opt/voxterm/libexec/models/|ls ~/.local/share/voxterm/models/|g" "$README"
fi

# Show diff
echo ""
echo "Changes:"
if [[ -f "$README" ]]; then
    git diff "$FORMULA" "$README"
else
    git diff "$FORMULA"
fi
echo ""

if [[ -f "$README" ]]; then
    git diff --quiet "$FORMULA" "$README" && {
        echo "No changes needed. Formula is already up to date."
        exit 0
    }
else
    git diff --quiet "$FORMULA" && {
        echo "No changes needed. Formula is already up to date."
        exit 0
    }
fi

# Commit and push
read -p "Commit and push these changes? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    if [[ -f "$README" ]]; then
        git add "$FORMULA" "$README"
    else
        git add "$FORMULA"
    fi
    git commit -m "Update to v$VERSION"
    git push origin main
    echo ""
    echo "=== Homebrew tap updated ==="
    echo "Users can now run: brew update && brew upgrade voxterm"
else
    echo "Changes not committed. Run manually:"
    echo "  cd $HOMEBREW_REPO"
    echo "  git add Formula/voxterm.rb"
    echo "  git commit -m 'Update to v$VERSION'"
    echo "  git push origin main"
fi
