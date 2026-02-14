"""devctl check command implementation."""

import subprocess
from datetime import datetime
from typing import List
from types import SimpleNamespace

from ..common import build_env, pipe_output, run_cmd, should_emit_output, write_output
from ..config import REPO_ROOT, SRC_DIR
from ..steps import format_steps_md
from .mutation_score import build_mutation_score_cmd, resolve_outcomes_path
from .mutants import build_mutants_cmd


def run(args) -> int:
    """Run the configured check profile and return exit code."""
    env = build_env(args)
    steps: List[dict] = []

    skip_build = args.skip_build
    skip_tests = args.skip_tests
    with_perf = args.with_perf
    with_mem_loop = args.with_mem_loop
    with_mutants = args.with_mutants
    with_mutation_score = args.with_mutation_score

    if args.profile == "ci":
        skip_build = True
        with_perf = False
        with_mem_loop = False
        with_mutants = False
        with_mutation_score = False
    elif args.profile == "prepush":
        with_perf = True
        with_mem_loop = True
    elif args.profile == "release":
        with_mutation_score = True
    elif args.profile == "quick":
        skip_build = True
        skip_tests = True

    def add_step(name: str, cmd: List[str], cwd=None) -> None:
        result = run_cmd(name, cmd, cwd=cwd, env=env, dry_run=args.dry_run)
        steps.append(result)
        if result["returncode"] != 0 and not args.keep_going:
            raise RuntimeError(f"{name} failed")

    try:
        if not args.skip_fmt:
            if args.fix:
                add_step("fmt", ["cargo", "fmt", "--all"], cwd=SRC_DIR)
            else:
                add_step(
                    "fmt-check",
                    ["cargo", "fmt", "--all", "--", "--check"],
                    cwd=SRC_DIR,
                )
        if not args.skip_clippy:
            add_step(
                "clippy",
                ["cargo", "clippy", "--workspace", "--all-features", "--", "-D", "warnings"],
                cwd=SRC_DIR,
            )
        if not skip_tests:
            add_step("test", ["cargo", "test", "--workspace", "--all-features"], cwd=SRC_DIR)
        if not skip_build:
            add_step(
                "build-release",
                ["cargo", "build", "--release", "--bin", "voiceterm"],
                cwd=SRC_DIR,
            )
        if with_perf:
            add_step(
                "perf-smoke",
                [
                    "cargo",
                    "test",
                    "--no-default-features",
                    "legacy_tui::tests::perf_smoke_emits_voice_metrics",
                    "--",
                    "--nocapture",
                ],
                cwd=SRC_DIR,
            )
            if not args.dry_run:
                log_path = subprocess.check_output(
                    [
                        "python3",
                        "-c",
                        "import os, tempfile; print(os.path.join(tempfile.gettempdir(), 'voiceterm_tui.log'))",
                    ],
                    text=True,
                ).strip()
                add_step(
                    "perf-verify",
                    ["python3", ".github/scripts/verify_perf_metrics.py", log_path],
                    cwd=REPO_ROOT,
                )
        if with_mem_loop:
            iterations = args.mem_iterations
            for i in range(iterations):
                add_step(
                    f"mem-guard-{i+1}",
                    [
                        "cargo",
                        "test",
                        "--no-default-features",
                        "legacy_tui::tests::memory_guard_backend_threads_drop",
                        "--",
                        "--nocapture",
                    ],
                    cwd=SRC_DIR,
                )
        if with_mutants:
            mutants_args = SimpleNamespace(
                all=args.mutants_all,
                module=args.mutants_module,
                timeout=args.mutants_timeout,
                shard=args.mutants_shard,
                results_only=False,
                json=False,
                offline=args.mutants_offline,
                cargo_home=args.mutants_cargo_home,
                cargo_target_dir=args.mutants_cargo_target_dir,
                plot=args.mutants_plot,
                plot_scope=args.mutants_plot_scope,
                plot_top_pct=args.mutants_plot_top_pct,
                plot_output=args.mutants_plot_output,
                plot_show=args.mutants_plot_show,
                top=None,
            )
            add_step("mutants", build_mutants_cmd(mutants_args), cwd=REPO_ROOT)
        if with_mutation_score:
            outcomes_path = resolve_outcomes_path(args.mutation_score_path)
            if outcomes_path is None:
                raise RuntimeError("mutation outcomes.json not found")
            add_step(
                "mutation-score",
                build_mutation_score_cmd(outcomes_path, args.mutation_score_threshold),
                cwd=REPO_ROOT,
            )
    except RuntimeError:
        pass

    success = all(step["returncode"] == 0 for step in steps)
    report = {
        "command": "check",
        "timestamp": datetime.now().isoformat(),
        "success": success,
        "steps": steps,
    }

    output = None
    if should_emit_output(args):
        if args.format == "json":
            output = json_dumps(report)
        elif args.format == "md":
            output = "# devctl check\n\n" + format_steps_md(steps)
        else:
            output = json_dumps(report)
        if args.output or args.format != "text":
            write_output(output, args.output)
        if args.pipe_command:
            pipe_rc = pipe_output(output, args.pipe_command, args.pipe_args)
            if pipe_rc != 0:
                return pipe_rc
    return 0 if success else 1


def json_dumps(payload: dict) -> str:
    import json

    return json.dumps(payload, indent=2)
