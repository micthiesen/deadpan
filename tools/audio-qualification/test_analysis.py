"""Headless tests of independent DSP measurement and failure detection."""
from array import array
import copy
import importlib.util
import math
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("audio_qualification", Path(__file__).with_name("run.py"))
qualification = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qualification)


class MeasurementTests(unittest.TestCase):
    def test_frequency_tracks_known_pitch_and_rejects_an_octave_error(self):
        def tone(hz):
            return [math.sin(2 * math.pi * hz * n / 48000) for n in range(9600)]
        expected = 440 * 2 ** (7 / 12)
        measured = qualification.frequency(tone(expected))
        self.assertLess(abs(1200 * math.log2(measured / expected)), .01)
        mismatch = qualification.frequency(tone(expected * 2))
        self.assertGreater(abs(1200 * math.log2(mismatch / expected)), qualification.TARGETS["tone_pitch_cents_max"])

    def test_length_and_nan_fail_instead_of_silently_filling_missing_output(self):
        with tempfile.TemporaryDirectory(prefix="deadpan-audio-test-") as directory:
            pcm = Path(directory) / "samples.f32"
            pcm.write_bytes(array("f", [0, 0, 1, -1]).tobytes())
            self.assertEqual(len(qualification.read_pcm(pcm, 2)), 4)
            with self.assertRaises(ValueError):
                qualification.read_pcm(pcm, 3)
            pcm.write_bytes(array("f", [0, float("nan")]).tobytes())
            with self.assertRaises(ValueError):
                qualification.read_pcm(pcm, 1)

    def test_dynamics_and_channel_swap_are_measurable(self):
        quiet_db = qualification.decibels_ratio(.06, .6)
        self.assertAlmostEqual(quiet_db, -20)
        actual_ratio = qualification.decibels_ratio(.6, .15)
        expected_ratio = qualification.decibels_ratio(.15, .6)
        self.assertGreater(abs(actual_ratio - expected_ratio), qualification.TARGETS["channel_ratio_error_db_max"])

    def test_difference_detects_shift_and_inverted_phase(self):
        reference = array("f", [0, 1, 0, -1])
        self.assertEqual(qualification.normalized_difference(reference, reference), 0)
        self.assertEqual(qualification.normalized_difference(reference, array("f", [-x for x in reference])), 2)
        self.assertGreater(qualification.normalized_difference(reference, array("f", [1, 0, -1, 0])), 1)
        with self.assertRaises(ValueError):
            qualification.normalized_difference(reference, array("f", [0]))

    def test_failed_target_remains_failed(self):
        assertions = []
        qualification.check(assertions, "deliberate_mismatch", "fixture", False, 2, .001)
        self.assertFalse(assertions[0]["passed"])
        self.assertEqual(assertions[0]["measured"], 2)

    def test_every_render_and_reset_phase_rejects_nonzero_allocations(self):
        case = {
            "name": "fixture", "exact": {"new_calls": 0}, "reset_call": {"new_calls": 0},
            **{mode: {phase: {"new_calls": 0} for phase in ("seek", "process", "flush")}
               for mode in ("blocks", "alternate_blocks", "reset_render")},
            "seeks": [{"target_input_sample": target, "render": {"new_calls": 0}} for target in range(4)],
        }
        paths = [("exact",), ("reset_call",)]
        paths += [(mode, phase) for mode in ("blocks", "alternate_blocks", "reset_render") for phase in ("seek", "process", "flush")]
        paths += [("seeks", index, "render") for index in range(4)]
        baseline = []
        qualification.check_phase_allocations(baseline, case)
        self.assertEqual(len(baseline), len(paths))
        self.assertTrue(all(item["passed"] for item in baseline))
        for path in paths:
            with self.subTest(phase=path):
                changed = copy.deepcopy(case)
                stats = changed
                for key in path:
                    stats = stats[key]
                stats["new_calls"] = 1
                assertions = []
                qualification.check_phase_allocations(assertions, changed)
                failed = [item for item in assertions if not item["passed"]]
                self.assertEqual(len(failed), 1)
                self.assertEqual(failed[0]["measured"], 1)
                self.assertEqual(failed[0]["target"], 0)
                # Prove the configured bound, rather than a hardcoded zero, is
                # used on every phase. The production target remains zero.
                with patch.dict(qualification.TARGETS, {"steady_cpp_new_calls_max": 1}):
                    assertions = []
                    qualification.check_phase_allocations(assertions, changed)
                    self.assertTrue(all(item["passed"] for item in assertions))
                    self.assertTrue(all(item["target"] == 1 for item in assertions))


if __name__ == "__main__":
    unittest.main()
