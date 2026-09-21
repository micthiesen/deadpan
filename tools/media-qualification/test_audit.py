"""Regression checks for rejecting mutable upstream source before any build."""

import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

import audit_upstream


class CheckoutValidation(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="deadpan-media-audit-test-", dir="/tmp")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.clone = self.root / "cutlass"
        self.clone.mkdir()
        self.git("init")
        (self.clone / "README.md").write_text("committed source\n")
        (self.clone / ".gitignore").write_text("target/\n")
        self.git("add", "README.md", ".gitignore")
        self.git("-c", "user.name=Deadpan Harness", "-c", "user.email=harness@example.invalid",
                 "-c", "core.hooksPath=/dev/null", "commit", "-m", "Create isolated source fixture")
        self.revision = self.git("rev-parse", "HEAD").stdout.strip()
        self.media = self.root / "media.json"
        self.media.write_text('{"cases":[]}\n')

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.clone, text=True, capture_output=True, check=True)

    def audit(self):
        report = self.root / "report.json"
        real_run = subprocess.run
        attempted_builds = []

        def stop_before_build(argv, **kwargs):
            if argv[0] == "cargo":
                attempted_builds.append(argv)
                raise RuntimeError("test stopped before dependency build")
            return real_run(argv, **kwargs)

        argv = ["audit_upstream.py", "--existing-clones", str(self.root), "--media-report", str(self.media),
                "--output", str(report)]
        with mock.patch.object(audit_upstream, "PINS", {"cutlass": ("unused", self.revision)}), \
                mock.patch.object(sys, "argv", argv), \
                mock.patch.object(audit_upstream.subprocess, "run", side_effect=stop_before_build), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(audit_upstream.main(), 1)
        return json.loads(report.read_text()), attempted_builds

    def test_tracked_modification_is_rejected_before_build(self):
        (self.clone / "README.md").write_text("modified source\n")
        report, builds = self.audit()
        self.assertEqual(builds, [])
        self.assertIn("tracked or untracked changes", report["failures"][0]["harness_error"])

    def test_untracked_source_is_rejected_before_build(self):
        (self.clone / "additional.rs").write_text("untracked source\n")
        report, builds = self.audit()
        self.assertEqual(builds, [])
        self.assertIn("additional.rs", report["source_checkout_checks"]["cutlass"]["tracked_or_untracked_changes"])

    def test_ignored_build_output_is_reported_and_excluded(self):
        (self.clone / "target").mkdir()
        (self.clone / "target/old.o").write_bytes(b"old artifact")
        report, builds = self.audit()
        self.assertEqual(len(builds), 1)
        check = report["source_checkout_checks"]["cutlass"]
        self.assertEqual(check["tracked_or_untracked_changes"], "")
        self.assertIn("target/", check["ignored_paths_excluded"])
        with tarfile.open(Path(report["work_directory"]) / "cutlass.tar") as archive:
            self.assertNotIn("target/old.o", archive.getnames())
        self.assertFalse((Path(report["work_directory"]) / "cutlass/target").exists())


if __name__ == "__main__":
    unittest.main()
