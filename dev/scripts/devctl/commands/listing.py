"""devctl list command implementation."""

import json
from datetime import datetime

from ..common import write_output

COMMANDS = [
    "check",
    "mutants",
    "mutation-score",
    "docs-check",
    "hygiene",
    "release",
    "homebrew",
    "status",
    "report",
    "list",
]

PROFILES = ["ci", "prepush", "release", "quick"]


def run(args) -> int:
    """List devctl commands and profiles."""
    payload = {
        "command": "list",
        "timestamp": datetime.now().isoformat(),
        "commands": COMMANDS,
        "profiles": PROFILES,
    }
    if args.format == "json":
        output = json.dumps(payload, indent=2)
    else:
        lines = ["# devctl list", "", "Commands:"]
        for name in COMMANDS:
            lines.append(f"- {name}")
        lines.append("")
        lines.append("Profiles:")
        for name in PROFILES:
            lines.append(f"- {name}")
        output = "\n".join(lines)
    write_output(output, args.output)
    return 0
