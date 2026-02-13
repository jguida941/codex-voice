"""devctl CLI argument parsing and dispatch."""

import argparse
import sys

from .commands import check, docs_check, homebrew, hygiene, listing, mutation_score, mutants, release, report, status
from .config import DEFAULT_CI_LIMIT, DEFAULT_MEM_ITERATIONS, DEFAULT_MUTANTS_TIMEOUT, DEFAULT_MUTATION_THRESHOLD


def build_parser() -> argparse.ArgumentParser:
    """Create the top-level argparse parser for devctl."""
    parser = argparse.ArgumentParser(description="VoxTerm Dev CLI")
    sub = parser.add_subparsers(dest="command", required=True)

    # check
    check_cmd = sub.add_parser("check", help="Run fmt/clippy/tests/build (and optional extras)")
    check_cmd.add_argument("--profile", choices=["ci", "prepush", "release", "quick"])
    check_cmd.add_argument("--ci", action="store_true", help="Match rust_ci.yml scope (alias for --profile ci)")
    check_cmd.add_argument("--prepush", action="store_true", help="Run CI + perf/mem loop (alias for --profile prepush)")
    check_cmd.add_argument("--skip-fmt", action="store_true")
    check_cmd.add_argument("--skip-clippy", action="store_true")
    check_cmd.add_argument("--skip-tests", action="store_true")
    check_cmd.add_argument("--skip-build", action="store_true")
    check_cmd.add_argument("--fix", action="store_true", help="Run cargo fmt (not --check)")
    check_cmd.add_argument("--with-perf", action="store_true", help="Run perf smoke + verify")
    check_cmd.add_argument("--with-mem-loop", action="store_true", help="Run memory guard loop")
    check_cmd.add_argument("--mem-iterations", type=int, default=DEFAULT_MEM_ITERATIONS)
    check_cmd.add_argument("--with-mutants", action="store_true", help="Run mutants after checks")
    check_cmd.add_argument("--with-mutation-score", action="store_true", help="Check mutation score")
    check_cmd.add_argument("--mutation-score-path", help="Path to outcomes.json")
    check_cmd.add_argument("--mutation-score-threshold", type=float, default=DEFAULT_MUTATION_THRESHOLD)
    check_cmd.add_argument("--mutants-module", help="Mutants module filter")
    check_cmd.add_argument("--mutants-all", action="store_true")
    check_cmd.add_argument("--mutants-timeout", type=int, default=DEFAULT_MUTANTS_TIMEOUT)
    check_cmd.add_argument("--mutants-offline", action="store_true")
    check_cmd.add_argument("--mutants-cargo-home")
    check_cmd.add_argument("--mutants-cargo-target-dir")
    check_cmd.add_argument("--mutants-plot", action="store_true")
    check_cmd.add_argument("--mutants-plot-scope", choices=["file", "dir"])
    check_cmd.add_argument("--mutants-plot-top-pct", type=float)
    check_cmd.add_argument("--mutants-plot-output")
    check_cmd.add_argument("--mutants-plot-show", action="store_true")
    check_cmd.add_argument("--keep-going", action="store_true")
    check_cmd.add_argument("--dry-run", action="store_true")
    check_cmd.add_argument("--format", choices=["text", "json", "md"], default="text")
    check_cmd.add_argument("--output", help="Write report to a file")
    check_cmd.add_argument("--pipe-command", help="Pipe report output to a command")
    check_cmd.add_argument("--pipe-args", nargs="*", help="Extra args for pipe command")
    check_cmd.add_argument("--offline", action="store_true")
    check_cmd.add_argument("--cargo-home")
    check_cmd.add_argument("--cargo-target-dir")

    # mutants
    mutants_cmd = sub.add_parser("mutants", help="Run mutation testing helper")
    mutants_cmd.add_argument("--all", action="store_true")
    mutants_cmd.add_argument("--module")
    mutants_cmd.add_argument("--timeout", type=int, default=DEFAULT_MUTANTS_TIMEOUT)
    mutants_cmd.add_argument("--results-only", action="store_true")
    mutants_cmd.add_argument("--json", action="store_true")
    mutants_cmd.add_argument("--offline", action="store_true")
    mutants_cmd.add_argument("--cargo-home")
    mutants_cmd.add_argument("--cargo-target-dir")
    mutants_cmd.add_argument("--plot", action="store_true")
    mutants_cmd.add_argument("--plot-scope", choices=["file", "dir"])
    mutants_cmd.add_argument("--plot-top-pct", type=float)
    mutants_cmd.add_argument("--plot-output")
    mutants_cmd.add_argument("--plot-show", action="store_true")
    mutants_cmd.add_argument("--top", type=int)
    mutants_cmd.add_argument("--dry-run", action="store_true")

    # mutation score
    score_cmd = sub.add_parser("mutation-score", help="Check mutation score threshold")
    score_cmd.add_argument("--path", help="Path to outcomes.json (optional)")
    score_cmd.add_argument("--threshold", type=float, default=DEFAULT_MUTATION_THRESHOLD)
    score_cmd.add_argument("--dry-run", action="store_true")

    # release
    release_cmd = sub.add_parser("release", help="Run release.sh (optional homebrew)")
    release_cmd.add_argument("--version", required=True)
    release_cmd.add_argument("--homebrew", action="store_true")
    release_cmd.add_argument("--yes", action="store_true", help="Skip confirmation prompts")
    release_cmd.add_argument("--allow-ci", action="store_true")
    release_cmd.add_argument("--dry-run", action="store_true")

    # homebrew
    homebrew_cmd = sub.add_parser("homebrew", help="Run update-homebrew.sh")
    homebrew_cmd.add_argument("--version", required=True)
    homebrew_cmd.add_argument("--yes", action="store_true", help="Skip confirmation prompts")
    homebrew_cmd.add_argument("--allow-ci", action="store_true")
    homebrew_cmd.add_argument("--dry-run", action="store_true")

    # docs-check
    docs_cmd = sub.add_parser("docs-check", help="Verify user-facing docs are updated")
    docs_cmd.add_argument("--user-facing", action="store_true", help="Enforce user-facing doc updates")
    docs_cmd.add_argument("--strict", action="store_true", help="Require all user docs when --user-facing")
    docs_cmd.add_argument("--format", choices=["json", "md"], default="md")
    docs_cmd.add_argument("--output")
    docs_cmd.add_argument("--pipe-command", help="Pipe report output to a command")
    docs_cmd.add_argument("--pipe-args", nargs="*", help="Extra args for pipe command")

    # status
    status_cmd = sub.add_parser("status", help="Summarize git + mutation status")
    status_cmd.add_argument("--ci", action="store_true", help="Include recent GitHub runs")
    status_cmd.add_argument("--ci-limit", type=int, default=DEFAULT_CI_LIMIT)
    status_cmd.add_argument("--format", choices=["json", "md", "text"], default="text")
    status_cmd.add_argument("--output")
    status_cmd.add_argument("--pipe-command", help="Pipe report output to a command")
    status_cmd.add_argument("--pipe-args", nargs="*", help="Extra args for pipe command")

    # report
    report_cmd = sub.add_parser("report", help="Generate a JSON/MD report")
    report_cmd.add_argument("--ci", action="store_true", help="Include recent GitHub runs")
    report_cmd.add_argument("--ci-limit", type=int, default=DEFAULT_CI_LIMIT)
    report_cmd.add_argument("--format", choices=["json", "md"], default="md")
    report_cmd.add_argument("--output")
    report_cmd.add_argument("--pipe-command", help="Pipe report output to a command")
    report_cmd.add_argument("--pipe-args", nargs="*", help="Extra args for pipe command")

    # list
    list_cmd = sub.add_parser("list", help="List devctl commands and profiles")
    list_cmd.add_argument("--format", choices=["json", "md"], default="md")
    list_cmd.add_argument("--output")

    # hygiene
    hygiene_cmd = sub.add_parser("hygiene", help="Audit archive/ADR/scripts governance hygiene")
    hygiene_cmd.add_argument("--format", choices=["json", "md"], default="md")
    hygiene_cmd.add_argument("--output")
    hygiene_cmd.add_argument("--pipe-command", help="Pipe report output to a command")
    hygiene_cmd.add_argument("--pipe-args", nargs="*", help="Extra args for pipe command")

    return parser


def main() -> int:
    """Entry point for devctl CLI."""
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "check":
        if args.ci:
            args.profile = "ci"
        if args.prepush:
            args.profile = "prepush"
        return check.run(args)
    if args.command == "mutants":
        return mutants.run(args)
    if args.command == "mutation-score":
        return mutation_score.run(args)
    if args.command == "docs-check":
        return docs_check.run(args)
    if args.command == "release":
        return release.run(args)
    if args.command == "homebrew":
        return homebrew.run(args)
    if args.command == "status":
        return status.run(args)
    if args.command == "report":
        return report.run(args)
    if args.command == "list":
        return listing.run(args)
    if args.command == "hygiene":
        return hygiene.run(args)

    return 0


if __name__ == "__main__":
    sys.exit(main())
