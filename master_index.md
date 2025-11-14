# Master Index

Central navigation guide for the Codex Voice project. Update this file at the end of each working day so every new directory, document, or architectural note remains discoverable.

## Core Governance Files

- `agents.md` — SDLC, coding, and workflow requirements that every agent must follow before touching code.
- `PROJECT_OVERVIEW.md` — High-level goals, current focus areas, and links into the dated architecture folders (updated whenever the “latest” folder changes).
- `master_index.md` — This file. Lists every major directory and document for quick orientation.
- `CHANGELOG.md` — Repository-wide history of notable changes (must be updated in every PR).

## Top-Level Directories

- `docs/` — Reference material, developer guides, audit notes. Daily architecture folders live under `docs/architecture/YYYY-MM-DD/` (create one per working day with `ARCHITECTURE.md`, daily `CHANGELOG.md`, diagrams, etc.).
  - Latest daily notes: [`docs/architecture/2025-11-13/`](docs/architecture/2025-11-13/)
    - Benchmarks + metrics evidence: [`docs/architecture/2025-11-13/BENCHMARKS.md`](docs/architecture/2025-11-13/BENCHMARKS.md)
  - Previous day baseline: [`docs/architecture/2025-11-12/`](docs/architecture/2025-11-12/)
  - References live in [`docs/references/`](docs/references/) (developer workflows, how-to documents such as `quick_start.md`, `testing.md`, `python_legacy.md`, `milestones.md`, `troubleshooting.md`).
  - Archived legacy references: [`docs/archive/OBSOLETE_REFERENCES_2025-11-12/`](docs/archive/OBSOLETE_REFERENCES_2025-11-12/).
  - External/third-party audits live in [`docs/audits/`](docs/audits/) (e.g., `READINESS_AUDIT_2025-11-12.md`, `claudeaudit.md`, `2025-11-12-chatgpt.md`).
- `rust_tui/` — Primary Rust workspace containing the TUI wrapper (Cargo project). Includes source (`src/`), docs, scripts, and tests tied to the Rust implementation.
  - Notable modules: `src/app.rs` (TUI state), `src/ui.rs` (render loop), the new `src/codex.rs` async worker for Codex calls, and `src/voice.rs` (voice/STT worker).
- `voice/` — Legacy or auxiliary voice assets (verify contents before use).
- `models/` — Whisper/STT model artifacts or pointers.
- `scripts/` — Helper scripts (PTY helpers, automation utilities).
- `test/`, `tst/`, `stubs/` — Python prototype tests, fixtures, or stub data (confirm before editing).
- `__pycache__/`, `bt.log`, `codex_voice.py`, `test_input.txt` — Python prototype artifacts/logs (retain for historical reference unless superseded by Rust path).

## Key Documents

- `README.md` — 5-minute quick start commands (build, run, controls).
- `PROJECT_OVERVIEW.md` — Canonical roadmap + link to the current daily architecture folder (with “You Are Here”).
- `docs/architecture/YYYY-MM-DD/` — Daily notes (latest: `docs/architecture/2025-11-13/`; previous: `docs/architecture/2025-11-12/`).
- `docs/references/` — Living references (e.g., `quick_start.md`, `testing.md`, `python_legacy.md`, `milestones.md`, `troubleshooting.md`).
- `docs/references/cicd_plan.md` — Canonical CI/CD blueprint covering architecture, dependencies, and the phased implementation plan.
- `docs/audits/` — External/internal audits (e.g., `claudeaudit.md`, `claude_audit_nov12.md`, `2025-11-12-chatgpt.md`).

## Daily Update Checklist

1. Add or update the current day’s `docs/architecture/YYYY-MM-DD/` folder with `ARCHITECTURE.md`, daily `CHANGELOG.md`, and supporting artifacts.
2. Update `PROJECT_OVERVIEW.md` to point to the newest daily folder.
3. Append any new directories or documents to this `master_index.md`, with a short description.
4. Ensure `agents.md` still reflects the latest governance rules.
5. Confirm CI will see the architectural note + changelog updates before merging.
6. Verify no standalone architecture files were added outside `docs/architecture/YYYY-MM-DD/`.

Following this workflow keeps the repository navigable and enforces the traceability requirements described in `agents.md`.
