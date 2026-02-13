"""Tests for devctl hygiene auditing."""

import tempfile
import unittest
from pathlib import Path

from dev.scripts.devctl.commands import hygiene


class HygieneAuditTests(unittest.TestCase):
    """Validate core hygiene audit checks."""

    def setUp(self) -> None:
        self.original_repo_root = hygiene.REPO_ROOT

    def tearDown(self) -> None:
        hygiene.REPO_ROOT = self.original_repo_root

    def test_archive_audit_flags_bad_filename(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive_dir = root / "dev/archive"
            archive_dir.mkdir(parents=True)
            (archive_dir / "README.md").write_text("# Archive\n", encoding="utf-8")
            (archive_dir / "bad-name.md").write_text("x", encoding="utf-8")
            hygiene.REPO_ROOT = root

            report = hygiene._audit_archive()

            self.assertTrue(report["errors"])
            self.assertIn("bad-name.md", report["errors"][0])

    def test_adr_audit_flags_index_status_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            adr_dir = root / "dev/adr"
            adr_dir.mkdir(parents=True)
            (adr_dir / "0001-example.md").write_text(
                "# ADR 0001: Example\n\nStatus: Accepted\nDate: 2026-02-13\n",
                encoding="utf-8",
            )
            (adr_dir / "README.md").write_text(
                "# ADR Index\n\n| ADR | Title | Status |\n|-----|-------|--------|\n"
                "| [0001](0001-example.md) | Example | Proposed |\n",
                encoding="utf-8",
            )
            hygiene.REPO_ROOT = root

            report = hygiene._audit_adrs()

            self.assertTrue(report["errors"])
            self.assertTrue(any("status mismatch" in error for error in report["errors"]))

    def test_scripts_audit_flags_undocumented_top_level_script(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            scripts_dir = root / "dev/scripts"
            scripts_dir.mkdir(parents=True)
            (scripts_dir / "README.md").write_text("# Scripts\n", encoding="utf-8")
            (scripts_dir / "new_tool.sh").write_text("#!/usr/bin/env bash\n", encoding="utf-8")
            hygiene.REPO_ROOT = root

            report = hygiene._audit_scripts()

            self.assertTrue(report["errors"])
            self.assertIn("new_tool.sh", report["errors"][0])


if __name__ == "__main__":
    unittest.main()
