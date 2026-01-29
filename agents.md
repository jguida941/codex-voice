# Agents

This file defines the SDLC/release expectations for this repo. Any user-facing change must
follow these steps before shipping.

## SDLC expectations
- Update docs for user-facing behavior changes (at least `README.md` and `QUICK_START.md`).
- Add or update an entry in `docs/CHANGELOG.md` with a clear summary and the correct date.
- For releases, bump `rust_tui/Cargo.toml` version and align docs with the new version.
- Run the appropriate verification (at minimum a local build of `codex-voice`).
- Keep UX tables/controls lists in sync with the actual behavior.

## Homebrew tap
- Tap repo: `https://github.com/jguida941/homebrew-codex-voice`
- After a release/version bump, update the formula there (version + checksum) and push it.
- Verify a fresh install works (`brew install` or `brew reinstall`) after updating the tap.

## Notes
- `docs/CHANGELOG.md` references this file as the SDLC policy source.
- If UI output or flags change, update any screenshots or tables that mention them.
