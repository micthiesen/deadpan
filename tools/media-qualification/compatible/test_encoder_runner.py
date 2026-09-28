"""Command retention failures are tested without a codec, compiler or GPU."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from qualify_encoder import EncoderHarness, read_pcm


class EncoderRunnerTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        report = self.root / "build.json"
        report.write_text(json.dumps({"prefix": str(self.root / "prefix")}))
        self.harness = EncoderHarness(self.root, False, report)

    def test_long_success_output_is_retained_completely(self):
        result = self.harness.run([sys.executable, "-c", "print('x' * 10000)"])
        self.assertEqual(result.stdout, "x" * 10000 + "\n")
        record = self.harness.report["commands"][0]
        self.assertEqual(Path(record["logs"]["stdout"]["path"]).read_text(), result.stdout)
        self.assertEqual(record["logs"]["stdout"]["bytes"], 10001)
        self.assertEqual(record["exit_code"], 0)

    def test_failed_optional_case_does_not_hide_following_result(self):
        result = self.harness.run([sys.executable, "-c", "import sys; print('failure',file=sys.stderr); sys.exit(9)"], required=False)
        self.assertEqual(result.returncode, 9)
        self.assertEqual(result.stderr, "failure\n")
        self.harness.run([sys.executable, "-c", "print('next case')"])
        self.assertEqual([row["exit_code"] for row in self.harness.report["commands"]], [9, 0])
        self.assertEqual(Path(self.harness.report["commands"][0]["logs"]["stderr"]["path"]).read_text(), "failure\n")

    def test_required_failure_retains_terminal_command(self):
        with self.assertRaisesRegex(RuntimeError, "failed \\(7\\)"):
            self.harness.run([sys.executable, "-c", "raise SystemExit(7)"])
        self.assertEqual(self.harness.report["commands"][0]["exit_code"], 7)

    def test_timeout_retains_partial_output_and_distinct_outcome(self):
        def timeout(argv, **kwargs):
            kwargs["stdout"].write(b"partial output\n")
            raise subprocess.TimeoutExpired(argv, kwargs["timeout"])
        with mock.patch("recorded_harness.subprocess.run", side_effect=timeout):
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                self.harness.run(["fixture"], timeout=1)
        record = self.harness.report["commands"][0]
        self.assertTrue(record["timed_out"])
        self.assertIsNone(record["exit_code"])
        self.assertEqual(Path(record["logs"]["stdout"]["path"]).read_text(), "partial output\n")
        self.assertEqual(self.harness.process_faults, [{"command": 0, "reason": "timeout"}])

    def test_launch_error_is_retained(self):
        with mock.patch("recorded_harness.subprocess.run", side_effect=FileNotFoundError("missing fixture")):
            with self.assertRaisesRegex(RuntimeError, "could not launch"):
                self.harness.run(["fixture"])
        record = self.harness.report["commands"][0]
        self.assertIsNone(record["exit_code"])
        self.assertIn("missing fixture", record["launch_error"])
        self.assertEqual(set(record["logs"]), {"stdout", "stderr"})

    def test_optional_sanitizer_failure_is_an_experiment_fault(self):
        self.harness.run([sys.executable, "-c", "import sys; print('ERROR: AddressSanitizer', file=sys.stderr); sys.exit(86)"], required=False)
        self.assertEqual(self.harness.process_faults, [{"command": 0, "reason": "signal or sanitizer failure"}])

    def test_changed_admitted_file_cannot_pass(self):
        path = self.root / "library"
        path.write_bytes(b"replacement")
        self.harness.admitted_files[str(path)] = "old hash"
        self.harness.report["result"] = "passed scoped encoder checks"
        self.harness.finish_admission()
        observation = next(row for row in self.harness.report["final_file_admission"] if row.get("path") == str(path))
        self.assertFalse(observation["passed"])
        self.assertTrue(self.harness.report["result"].startswith("failed:"))

    def test_loader_overrides_are_removed_from_child_environment(self):
        with mock.patch.dict("os.environ", {"DYLD_INSERT_LIBRARIES": "unadmitted", "LD_PRELOAD": "unadmitted"}):
            harness = EncoderHarness(self.root, False, self.root / "build.json")
        self.assertNotIn("DYLD_INSERT_LIBRARIES", harness.environment)
        self.assertNotIn("LD_PRELOAD", harness.environment)
        self.assertIn("DYLD_INSERT_LIBRARIES", harness.report["removed_loader_override_names"])

    def test_retargeted_load_path_cannot_hide_behind_unchanged_real_files(self):
        original = self.root / "original"
        replacement = self.root / "replacement"
        original.write_bytes(b"same bytes")
        replacement.write_bytes(b"same bytes")
        link = self.root / "libavcodec.62.dylib"
        link.symlink_to(original)
        from qualify_encoder import native
        self.harness.loaded_paths[str(link)] = {"resolved": str(original.resolve()), "sha256": native.digest(original)}
        link.unlink()
        link.symlink_to(replacement)
        self.harness.report["result"] = "passed scoped encoder checks"
        self.harness.finish_admission()
        self.assertFalse(self.harness.report["final_file_admission"][-1]["passed"])
        self.assertTrue(self.harness.report["result"].startswith("failed:"))

    def test_partial_stereo_frame_is_rejected(self):
        path = self.root / "partial.f32"
        path.write_bytes(bytes(4))
        with self.assertRaisesRegex(ValueError, "interleaved stereo"):
            read_pcm(path)


if __name__ == "__main__":
    unittest.main()
