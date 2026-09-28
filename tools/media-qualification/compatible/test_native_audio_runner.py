"""Native runner admission and reporting, with no compiler or media execution."""

import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from qualify_native_audio import (CASE_NAMES, MAX_INPUT, NativeAudioHarness, PASS_RESULT,
                                  SOURCE_NAMES, bounded_file, dependencies, require_fixture,
                                  rpaths, source_inventory, strict_json, system_dependency)
from recorded_harness import digest


class NativeRunnerTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.harness = NativeAudioHarness(self.root, False, self.root / "encoder.json")
        self.harness.encoder_work = self.root.resolve()
        self.harness.encoder_cases = [self.fixture(name) for name in CASE_NAMES]

    def fixture(self, name):
        path = self.root / (name + ".mp4")
        path.write_bytes(name.encode())
        edits = name.removeprefix("impulse-60-")
        return {"name": name, "encode_exit_code": 0, "scoped_checks_passed": False,
                "requested": {"mode": "hardware-no-b", "edit_lists": edits, "fps": [60, 1],
                              "frames": 120, "pcm": "impulses"},
                "source": {"schema_version": 1, "kind": "encode", "mode": "hardware-no-b",
                           "edit_lists": edits, "frame_count": 120, "frame_rate": [60, 1],
                           "time_base": [1, 60], "pcm_kind": "impulses", "audio_samples": 96000,
                           "audio_offset_samples": 0, "start_pts": 0, "duration_ticks": 120,
                           "width": 320, "height": 180, "requested_b_frames": 0,
                           "impulses": [100, 48000, 95800]},
                "artifacts": {"mp4": {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}}}

    def fake_probe(self, argv, **kwargs):
        sha = digest(Path(argv[1]))
        observation = {"status": "completed", "input_sha256": sha, "final_input_sha256": sha,
                       "stored": {"status": "completed"}, "pcm": {"status": "completed"}}
        Path(argv[2]).write_bytes(bytes(16))
        kwargs["stdout"].write(json.dumps(observation).encode())
        return subprocess.CompletedProcess(argv, 0)

    @staticmethod
    def oracle(outcome):
        def inspect(spec, observation, pcm, **kwargs):
            return {"passed": outcome != "failed", "outcome": outcome,
                    "event_timing_qualified": outcome == "passed", "checks": [],
                    "observations": {"raw_native_observation": observation},
                    "unqualified": ["retained uncertain trim"] if outcome == "unqualified" else []}
        return inspect

    def test_negative_encoder_timing_does_not_reject_exact_fixture(self):
        source = require_fixture(self.harness.encoder_cases[0], CASE_NAMES[0])
        self.assertEqual(source["audio_samples"], 96000)
        for key, replacement in (("frame_rate", [30000, 1001]), ("audio_offset_samples", 1024),
                                 ("frame_count", 1), ("start_pts", False)):
            changed = copy.deepcopy(self.harness.encoder_cases[0])
            changed["source"][key] = replacement
            with self.subTest(key=key), self.assertRaises(ValueError):
                require_fixture(changed, CASE_NAMES[0])

    def test_complete_pair_retains_raw_logs_pcm_and_full_oracle(self):
        with mock.patch("recorded_harness.subprocess.run", side_effect=self.fake_probe), \
             mock.patch("qualify_native_audio.inspect_native_audio", side_effect=self.oracle("passed")):
            self.harness.capture_all()
        self.assertEqual(self.harness.report["result"], PASS_RESULT)
        self.assertTrue(self.harness.report["experiment_completed"])
        for case in self.harness.report["cases"]:
            self.assertEqual(case["status"], "passed")
            self.assertEqual(case["pcm"]["bytes"], 16)
            self.assertEqual(case["pcm"]["sha256"], hashlib.sha256(bytes(16)).hexdigest())
            self.assertIn("stored", case["oracle"]["observations"]["raw_native_observation"])
            self.assertTrue(Path(case["raw_logs"]["stdout"]["path"]).is_file())
        self.assertTrue(all(row["timeout_seconds"] == 120 for row in self.harness.report["commands"]))

    def test_unqualified_is_completed_but_never_success(self):
        with mock.patch("recorded_harness.subprocess.run", side_effect=self.fake_probe), \
             mock.patch("qualify_native_audio.inspect_native_audio", side_effect=self.oracle("unqualified")):
            self.harness.capture_all()
        self.assertTrue(self.harness.report["experiment_completed"])
        self.assertNotEqual(self.harness.report["result"], PASS_RESULT)
        self.assertEqual([case["status"] for case in self.harness.report["cases"]], ["unqualified", "unqualified"])
        self.assertEqual(self.harness.report["cases"][0]["oracle"]["unqualified"], ["retained uncertain trim"])

    def test_bad_first_hash_preserves_following_case(self):
        self.harness.encoder_cases[0]["artifacts"]["mp4"]["sha256"] = "0" * 64
        with mock.patch("recorded_harness.subprocess.run", side_effect=self.fake_probe), \
             mock.patch("qualify_native_audio.inspect_native_audio", side_effect=self.oracle("passed")):
            self.harness.capture_all()
        self.assertEqual([case["status"] for case in self.harness.report["cases"]], ["failed", "passed"])
        self.assertIn("SHA-256 mismatch", self.harness.report["cases"][0]["failure"])
        self.assertEqual(len(self.harness.report["commands"]), 1)

    def test_duplicate_case_and_malformed_artifact_are_explicit_failures(self):
        self.harness.encoder_cases.append(copy.deepcopy(self.harness.encoder_cases[0]))
        self.harness.encoder_cases[1]["artifacts"] = []
        self.harness.capture_all()
        self.assertEqual(len(self.harness.report["cases"]), 2)
        self.assertIn("exactly one", self.harness.report["cases"][0]["failure"])
        self.assertIn("missing retained MP4", self.harness.report["cases"][1]["failure"])

    def test_reader_hash_must_match_admitted_bytes(self):
        def wrong(argv, **kwargs):
            kwargs["stdout"].write(json.dumps({"input_sha256": "0" * 64, "final_input_sha256": "0" * 64}).encode())
            return subprocess.CompletedProcess(argv, 0)
        with mock.patch("recorded_harness.subprocess.run", side_effect=wrong):
            case = self.harness.capture_case(CASE_NAMES[0])
        self.assertEqual(case["status"], "failed")
        self.assertIn("admitted input hash", case["failure"])
        self.assertIn("native_observation", case)

    def test_timeout_keeps_partial_log_and_allows_second_case(self):
        def run(argv, **kwargs):
            if CASE_NAMES[0] in argv[1]:
                kwargs["stderr"].write(b"partial native failure")
                raise subprocess.TimeoutExpired(argv, kwargs["timeout"])
            return self.fake_probe(argv, **kwargs)
        with mock.patch("recorded_harness.subprocess.run", side_effect=run), \
             mock.patch("qualify_native_audio.inspect_native_audio", side_effect=self.oracle("passed")):
            self.harness.capture_all()
        self.assertEqual([case["status"] for case in self.harness.report["cases"]], ["failed", "passed"])
        self.assertEqual(self.harness.report["result"], "failed: native process fault")
        self.assertEqual(Path(self.harness.report["cases"][0]["raw_logs"]["stderr"]["path"]).read_bytes(), b"partial native failure")

    def test_sanitizer_text_is_fault_even_with_zero_exit(self):
        def run(argv, **kwargs):
            kwargs["stderr"].write(b"runtime error: injected test")
            return self.fake_probe(argv, **kwargs)
        with mock.patch("recorded_harness.subprocess.run", side_effect=run), \
             mock.patch("qualify_native_audio.inspect_native_audio", side_effect=self.oracle("passed")):
            self.harness.capture_all()
        self.assertEqual(self.harness.report["result"], "failed: native process fault")

    def test_command_output_cap_is_failure_with_artifact(self):
        def excessive(argv, **kwargs):
            kwargs["stdout"].truncate(16 * 1024 * 1024 + 1)
            return subprocess.CompletedProcess(argv, 0)
        with mock.patch("recorded_harness.subprocess.run", side_effect=excessive):
            case = self.harness.capture_case(CASE_NAMES[0])
        self.assertEqual(case["status"], "failed")
        self.assertEqual(case["raw_logs"]["stdout"]["bytes"], 16 * 1024 * 1024 + 1)
        self.assertEqual(self.harness.process_faults[-1]["reason"], "output limit")

    def test_partial_pcm_is_rejected_without_hiding_raw_bytes(self):
        def partial(argv, **kwargs):
            result = self.fake_probe(argv, **kwargs)
            Path(argv[2]).write_bytes(bytes(4))
            return result
        with mock.patch("recorded_harness.subprocess.run", side_effect=partial):
            case = self.harness.capture_case(CASE_NAMES[0])
        self.assertEqual(case["status"], "failed")
        self.assertEqual(case["pcm"]["bytes"], 4)
        self.assertIn("partial interleaved stereo", case["failure"])

    def test_bounded_file_rejects_symlink_directory_and_oversize(self):
        path = self.root / "bounded"
        path.write_bytes(b"1234")
        with self.assertRaisesRegex(ValueError, "bounded regular"):
            bounded_file(path, 3)
        link = self.root / "link"
        link.symlink_to(path)
        with self.assertRaises(OSError):
            bounded_file(link, 4)
        with self.assertRaises(ValueError):
            bounded_file(self.root, 100)

    def test_report_read_cap_and_json_ambiguity(self):
        oversized = self.root / "encoder.json"
        with oversized.open("wb") as target:
            target.truncate(MAX_INPUT + 1)
        with self.assertRaisesRegex(ValueError, "bounded regular"):
            self.harness.load_encoder_report()
        for text in ('{"x":1,"x":2}', '{"x":NaN}', '{"x":1e999}'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                strict_json(text)

    def test_loader_and_inherited_sanitizer_overrides_are_removed(self):
        with mock.patch.dict("os.environ", {"DYLD_INSERT_LIBRARIES": "bad", "LD_PRELOAD": "bad",
                                           "__XPC_DYLD_LIBRARY_PATH": "bad", "ASAN_OPTIONS": "bad"}):
            clean = NativeAudioHarness(self.root, False, self.root / "encoder.json")
        self.assertTrue(all(key not in clean.environment for key in clean.report["removed_environment_override_names"]))

    def test_non_system_linkage_and_unrecorded_runtime_are_rejected(self):
        for dependency in ("/opt/homebrew/lib/libcodec.dylib", "@rpath/libclang_rt.asan_osx_dynamic.dylib",
                           "/usr/lib/../../tmp/libfake.dylib"):
            output = f"probe:\n\t{dependency} (compatibility version 1.0.0, current version 1.0.0)\n"
            with self.subTest(dependency=dependency), self.assertRaises(ValueError):
                self.harness.admit_linkage(self.harness.binary, output, [])
        self.assertTrue(system_dependency("/System/Library/Frameworks/AVFoundation.framework/Versions/A/AVFoundation"))

    def test_sanitizer_runtime_must_resolve_to_compiler_directory(self):
        self.harness.sanitizers = True
        runtime = self.root / "compiler/lib/darwin"
        runtime.mkdir(parents=True)
        name = "libclang_rt.asan_osx_dynamic.dylib"
        (runtime / name).write_bytes(b"compiler runtime")
        self.harness.runtime_directory = runtime
        output = f"probe:\n\t@rpath/{name} (compatibility version 0.0.0, current version 0.0.0)\n"
        records = self.harness.admit_linkage(self.harness.binary, output, [str(runtime)], runtime=True)
        self.assertEqual(records[0]["sha256"], digest(runtime / name))
        (self.root / name).write_bytes(b"shadow runtime")
        with self.assertRaisesRegex(ValueError, "selected compiler"):
            self.harness.admit_linkage(self.harness.binary, output, [str(self.root), str(runtime)], runtime=True)

    def test_changed_input_or_helper_cannot_retain_success(self):
        path = self.root / "admitted"
        path.write_bytes(b"old")
        self.harness.admit(path, 20)
        path.write_bytes(b"new")
        self.harness.report["result"] = PASS_RESULT
        self.harness.finish_admission()
        self.assertTrue(self.harness.report["result"].startswith("failed:"))
        inventory = source_inventory()
        self.assertEqual(set(inventory), set(SOURCE_NAMES))
        self.assertIn("avfoundation_probe.m", inventory)
        self.assertIn("recorded_harness.py", inventory)
        self.assertNotIn("run.py", inventory)

    def test_sanitizer_candidate_retarget_cannot_hide_behind_identical_bytes(self):
        self.harness.sanitizers = True
        runtime = self.root / "compiler/lib/darwin"
        runtime.mkdir(parents=True)
        name = "libclang_rt.asan_osx_dynamic.dylib"
        original = runtime / name
        original.write_bytes(b"same runtime bytes")
        replacement = self.root / "replacement.dylib"
        replacement.write_bytes(original.read_bytes())
        candidate = self.root / name
        candidate.symlink_to(original)
        self.harness.runtime_directory = runtime
        output = f"probe:\n\t@rpath/{name} (compatibility version 0.0.0, current version 0.0.0)\n"
        records = self.harness.admit_linkage(self.harness.binary, output, [str(self.root)], runtime=True)
        self.assertEqual(records[0]["candidate_path"], str(candidate))
        self.assertEqual(self.harness.loaded_paths[str(candidate)]["resolved"], str(original.resolve()))
        candidate.unlink()
        candidate.symlink_to(replacement)
        self.harness.report["result"] = PASS_RESULT
        self.harness.finish_admission()
        changed = next(row for row in self.harness.report["final_file_admission"]
                       if row.get("load_path") == str(candidate))
        self.assertFalse(changed["passed"])
        self.assertEqual(changed["expected"]["sha256"], changed["actual"]["sha256"])
        self.assertTrue(self.harness.report["result"].startswith("failed:"))

    def test_linkage_parser_keeps_exact_rpath_order(self):
        self.assertEqual(rpaths("cmd LC_RPATH\ncmdsize 32\npath @executable_path (offset 12)\n"
                                "cmd LC_RPATH\npath /compiler/lib (offset 12)\n"), ["@executable_path", "/compiler/lib"])
        with self.assertRaises(ValueError):
            dependencies("not an otool report")


if __name__ == "__main__":
    unittest.main()
