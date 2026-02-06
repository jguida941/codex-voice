# Contributing

Thanks for taking the time to contribute.

## Before you start

- For non-trivial changes, open or comment on an issue first so we can align on scope.
- Keep docs and UX tables/controls lists in sync with actual behavior.
- Update `dev/CHANGELOG.md` for user-facing changes.
- Run verification before shipping (see `dev/DEVELOPMENT.md` for the full checklist).

## Development setup

- Install prerequisites in `guides/INSTALL.md`.
- Build the overlay:
  ```bash
  cd src && cargo build --release --bin voxterm
  ```

## Code style

- Rust: `cargo fmt` and `cargo clippy --workspace --all-features -- -D warnings`.
- Keep changes focused; prefer small, reviewable commits.

## Tests

Run what matches your changes:

```bash
cd src && cargo test
```

For overlay-only changes:

```bash
cd src && cargo test --bin voxterm
```

Targeted checks mirrored in CI (run when relevant):

```bash
# Perf smoke (voice metrics)
cd src && cargo test --no-default-features legacy_tui::tests::perf_smoke_emits_voice_metrics -- --nocapture

# Memory guard (thread cleanup)
cd src && cargo test --no-default-features legacy_tui::tests::memory_guard_backend_threads_drop -- --nocapture

# Mutation testing (heavy; usually on demand)
cd src && cargo mutants --timeout 300 -o mutants.out
python3 ../dev/scripts/check_mutation_score.py --path mutants.out/outcomes.json --threshold 0.80
```

## Pull requests

- Explain the problem, the approach, and any tradeoffs.
- Include test output or notes on what was run.
- If UI output or flags change, update screenshots and docs that mention them.

## Security

For security concerns, see `.github/SECURITY.md`.
