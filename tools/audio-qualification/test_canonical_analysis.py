"""Regression tests for independent canonical adapter PCM analysis."""
from array import array
import copy
import importlib.util
import math
from pathlib import Path
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location(
    "canonical_audio_analysis", Path(__file__).with_name("canonical_analysis.py")
)
canonical = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(canonical)


def stats(dsp_calls=4):
    return {
        "calls": 4,
        "new_calls": 0,
        "new_bytes": 0,
        "milliseconds": 1.0,
        "maximum_call_ms": 0.25,
        "dsp_calls": dsp_calls,
        "maximum_buffer_frames": 256,
    }


def production_stats(dsp_calls=4):
    return {
        **stats(dsp_calls),
        "leading_output_frames": 0,
        "lookahead_input_frames": 0,
        "configure": {
            "calls": 1,
            "new_calls": 35,
            "new_bytes": 896_344,
            "milliseconds": 0.5,
            "maximum_call_ms": 0.5,
        },
    }


def nearest_even_ratio(value, numerator, denominator):
    whole, remainder = divmod(value * numerator, denominator)
    if remainder * 2 > denominator or (remainder * 2 == denominator and whole % 2):
        whole += 1
    return whole


def production_manifest():
    cases = []
    speeds = ((1, 2), (3, 4), (1, 1), (3, 2), (2, 1))

    def add_case(name, kind, input_file, input_frames, output_frames, pitch):
        starts = (
            output_frames * 4 // 5,
            output_frames // 7,
            output_frames - 1,
            output_frames // 2,
        )
        cases.append(
            {
                "name": name,
                "fixture_kind": kind,
                "input_file": input_file,
                "input_frames": input_frames,
                "output_frames": output_frames,
                "pitch_semitones": pitch,
                "renders": {
                    consumer: production_stats() for consumer in ("preview", "irregular", "export")
                },
                "seeks": [
                    {
                        "start_frame": start,
                        "frames": min(4800, output_frames - start),
                        "replay": production_stats(1),
                        "cached": production_stats(0),
                    }
                    for start in starts
                ],
            }
        )

    for numerator, denominator in speeds:
        for pitch in (-7, 0, 7):
            add_case(
                f"mixed-{numerator}-{denominator}-pitch-{pitch}",
                "mixed",
                "input.f32",
                192_192,
                nearest_even_ratio(192_192, denominator, numerator),
                pitch,
            )
    for input_frames in (31, 1003):
        for numerator, denominator in speeds:
            for pitch in (-7, 0, 7):
                add_case(
                    f"short-{input_frames}-{numerator}-{denominator}-pitch-{pitch}",
                    "impulse",
                    f"short-{input_frames}-input.f32",
                    input_frames,
                    nearest_even_ratio(input_frames, denominator, numerator),
                    pitch,
                )
    for input_frames in (1, 2, 5759, 5760, 5761):
        add_case(
            f"edge-{input_frames}",
            "impulse",
            f"edge-{input_frames}-input.f32",
            input_frames,
            input_frames,
            0,
        )
    return {
        "sample_rate": 48_000,
        "channels": 2,
        "quantum": 256,
        "analysis_window_ms": "120-15",
        "invalid_requests_rejected": 12,
        "cases": cases,
    }


class CanonicalAnalysisTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="deadpan-canonical-analysis-")
        self.addCleanup(temporary.cleanup)
        self.pcm = Path(temporary.name)

    def write(self, name, samples):
        (self.pcm / name).write_bytes(array("f", samples).tobytes())

    def impulse_fixture(self, frames=12, seek_start=3, seek_frames=6):
        samples = array("f", [0.0] * (frames * 2))
        peak = frames // 3
        samples[2 * peak] = 0.8
        samples[2 * peak + 1] = -0.2
        name = "impulse-unity"
        self.write("impulse-input.f32", samples)
        for consumer in ("preview", "irregular", "export"):
            self.write(f"{name}-{consumer}.f32", samples)
        excerpt = samples[2 * seek_start : 2 * (seek_start + seek_frames)]
        excerpt.extend([0.0] * (2 * seek_frames - len(excerpt)))
        for mode in ("replay", "cached"):
            self.write(f"{name}-{mode}-0.f32", excerpt)
        case = {
            "name": name,
            "fixture_kind": "impulse",
            "input_file": "impulse-input.f32",
            "input_frames": frames,
            "output_frames": frames,
            "pitch_semitones": 0,
            "renders": {consumer: stats() for consumer in ("preview", "irregular", "export")},
            "seeks": [
                {
                    "start_frame": seek_start,
                    "frames": seek_frames,
                    "replay": stats(1),
                    "cached": stats(0),
                }
            ],
        }
        return {"sample_rate": 48_000, "channels": 2, "cases": [case]}

    def failures(self, report, category):
        return [
            assertion
            for assertion in report["assertions"]
            if assertion["category"] == category and not assertion["passed"]
        ]

    def test_nominal_unity_impulse_and_seek_pass(self):
        report = canonical._analyze_cases(self.impulse_fixture(), self.pcm)
        self.assertEqual(report["assertions_failed"], 0)
        self.assertGreater(report["assertions_passed"], 0)
        self.assertEqual(report["measurements"][0]["outputs"]["preview"][0]["peak_frame"], 4)
        self.assertEqual(report["measurements"][0]["unity_identity_normalized_rms_error"], 0)

    def test_consumer_pcm_phase_difference_is_detected_from_files(self):
        native = self.impulse_fixture()
        samples = array("f")
        samples.frombytes((self.pcm / "impulse-unity-irregular.f32").read_bytes())
        self.write("impulse-unity-irregular.f32", [-sample for sample in samples])
        report = canonical._analyze_cases(native, self.pcm)
        failures = self.failures(report, "canonical_consumer_equivalence")
        self.assertEqual(len(failures), 1)
        self.assertIn("preview-vs-irregular", failures[0]["case"])
        self.assertFalse(failures[0]["measured"]["bit_exact"])

    def test_wrong_seek_content_and_out_of_range_window_are_detected(self):
        native = self.impulse_fixture()
        self.write("impulse-unity-replay-0.f32", [0.125] * 12)
        report = canonical._analyze_cases(native, self.pcm)
        self.assertEqual(len(self.failures(report, "seek_replay_equivalence")), 1)

        native = self.impulse_fixture(seek_start=7, seek_frames=6)
        report = canonical._analyze_cases(native, self.pcm)
        self.assertEqual(len(self.failures(report, "seek_window_within_render")), 1)
        self.assertEqual(len(self.failures(report, "seek_replay_equivalence")), 1)
        self.assertEqual(len(self.failures(report, "seek_cached_equivalence")), 1)

    def test_consumer_dependent_dsp_call_count_is_rejected(self):
        native = self.impulse_fixture()
        native["cases"][0]["renders"]["export"]["dsp_calls"] += 1
        report = canonical._analyze_cases(native, self.pcm)
        failures = self.failures(report, "consumer_dsp_schedule")
        self.assertEqual(len(failures), 1)
        self.assertIn("export", failures[0]["case"])

    def test_every_render_replay_and_cached_phase_enforces_resource_bounds(self):
        paths = [
            ("renders", consumer) for consumer in ("preview", "irregular", "export")
        ] + [("seeks", 0, mode) for mode in ("replay", "cached")]
        for path in paths:
            with self.subTest(path=path, violation="allocation"):
                native = self.impulse_fixture()
                phase = native["cases"][0]
                for key in path:
                    phase = phase[key]
                phase["new_calls"] = 1
                phase["new_bytes"] = 64
                report = canonical._analyze_cases(native, self.pcm)
                self.assertEqual(len(self.failures(report, "steady_cpp_new_calls")), 1)
                self.assertEqual(len(self.failures(report, "steady_cpp_new_bytes")), 1)
            with self.subTest(path=path, violation="buffer"):
                native = self.impulse_fixture()
                phase = native["cases"][0]
                for key in path:
                    phase = phase[key]
                phase["maximum_buffer_frames"] = 257
                report = canonical._analyze_cases(native, self.pcm)
                failures = self.failures(report, "bounded_audio_buffer")
                self.assertEqual(len(failures), 1)
                self.assertEqual(failures[0]["target"], 256)

    def test_silent_and_shifted_unity_impulses_fail_independent_audio_checks(self):
        native = self.impulse_fixture()
        silence = [0.0] * 24
        for consumer in ("preview", "irregular", "export"):
            self.write(f"impulse-unity-{consumer}.f32", silence)
        report = canonical._analyze_cases(native, self.pcm)
        self.assertEqual(len(self.failures(report, "impulse_nonzero_energy")), 6)
        self.assertEqual(len(self.failures(report, "unity_identity")), 1)

        native = self.impulse_fixture()
        shifted = array("f", [0.0] * 24)
        shifted[10] = 0.8
        shifted[11] = -0.2
        for consumer in ("preview", "irregular", "export"):
            self.write(f"impulse-unity-{consumer}.f32", shifted)
        report = canonical._analyze_cases(native, self.pcm)
        self.assertEqual(len(self.failures(report, "unity_impulse_onset")), 2)
        self.assertEqual(len(self.failures(report, "unity_identity")), 1)

    def test_mixed_fixture_uses_actual_frame_ratio_and_baseline_quality_targets(self):
        frames = 192_192
        samples = array("f")
        for frame in range(frames):
            time = frame / 48_000
            left = right = 0.0
            if 0.25 <= time < 1:
                left = 0.6 * math.sin(2 * math.pi * 440 * time)
                right = 0.15 * math.sin(2 * math.pi * 660 * time + 0.3)
            elif 1.25 <= time < 2:
                offset = time - 1.25
                left = 0.4 * math.sin(2 * math.pi * (200 * offset + 1200 * offset * offset))
                right = 0.1 * math.sin(2 * math.pi * (900 * offset - 400 * offset * offset))
            elif 2.5 <= time < 3:
                left = 0.1 * math.sin(2 * math.pi * 880 * time)
                right = 0.4 * math.sin(2 * math.pi * 880 * time)
            elif 3.25 <= time < 3.75:
                left = 0.06 * math.sin(2 * math.pi * 440 * time)
                right = 0.015 * math.sin(2 * math.pi * 660 * time + 0.3)
            if frame == 108_000:
                left = 0.8
            if frame == 110_400:
                right = -0.2
            samples.extend((left, right))
        name = "mixed-unity"
        self.write("input.f32", samples)
        for consumer in ("preview", "irregular", "export"):
            self.write(f"{name}-{consumer}.f32", samples)
        case = {
            "name": name,
            "fixture_kind": "mixed",
            "input_file": "input.f32",
            "input_frames": frames,
            "output_frames": frames,
            "pitch_semitones": 0,
            "renders": {consumer: stats(751) for consumer in ("preview", "irregular", "export")},
            "seeks": [],
        }
        report = canonical._analyze_cases(
            {"sample_rate": 48_000, "channels": 2, "cases": [case]}, self.pcm
        )
        self.assertEqual(report["assertions_failed"], 0)
        self.assertEqual(report["measurements"][0]["actual_rate"]["speed"], 1)

    def test_complete_production_manifest_is_accepted(self):
        canonical._validate_manifest(production_manifest(), "120-15")

    def test_empty_truncated_and_duplicate_case_manifests_are_rejected(self):
        empty = production_manifest()
        empty["cases"] = []
        truncated = production_manifest()
        truncated["cases"].pop()
        duplicate = production_manifest()
        duplicate["cases"][-1] = copy.deepcopy(duplicate["cases"][0])
        for label, native in (
            ("empty", empty),
            ("truncated", truncated),
            ("duplicate", duplicate),
        ):
            with self.subTest(label=label), self.assertRaises(ValueError):
                canonical._validate_manifest(native, "120-15")

    def test_case_recipe_and_input_file_must_match_pinned_matrix(self):
        mutations = {
            "name": "unrecognized-case",
            "fixture_kind": "mixed",
            "input_file": "different-input.f32",
            "input_frames": 32,
            "output_frames": 63,
            "pitch_semitones": 1,
        }
        for field, value in mutations.items():
            with self.subTest(field=field):
                native = production_manifest()
                native["cases"][15][field] = value
                with self.assertRaises(ValueError):
                    canonical._validate_manifest(native, "120-15")

    def test_missing_consumer_seek_and_stats_records_are_rejected(self):
        mutations = (
            lambda native: native["cases"][0]["renders"].pop("export"),
            lambda native: native["cases"][0]["seeks"].pop(),
            lambda native: native["cases"][0]["seeks"][0].pop("cached"),
            lambda native: native["cases"][0]["renders"]["preview"].pop("new_calls"),
        )
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index):
                native = production_manifest()
                mutate(native)
                with self.assertRaises(ValueError):
                    canonical._validate_manifest(native, "120-15")

    def test_seek_windows_must_match_pinned_probe_recipe(self):
        for field in ("start_frame", "frames"):
            with self.subTest(field=field):
                native = production_manifest()
                native["cases"][0]["seeks"][0][field] += 1
                with self.assertRaises(ValueError):
                    canonical._validate_manifest(native, "120-15")

    def test_requested_window_and_root_configuration_are_strict(self):
        native = production_manifest()
        with self.assertRaises(ValueError):
            canonical._validate_manifest(native, "120-30")
        with self.assertRaises(ValueError):
            canonical._validate_manifest(native, "unsupported")
        for field, value in (
            ("sample_rate", 44_100),
            ("channels", 1),
            ("quantum", 512),
            ("invalid_requests_rejected", 11),
            ("analysis_window_ms", "120-30"),
        ):
            with self.subTest(field=field):
                changed = production_manifest()
                changed[field] = value
                with self.assertRaises(ValueError):
                    canonical._validate_manifest(changed, "120-15")

    def test_malformed_analyze_input_raises_value_error_before_pcm_access(self):
        malformed = (
            None,
            [],
            {},
            {"bad": 0, 1: 2},
            {"sample_rate": 48_000},
            {**production_manifest(), "cases": "not-an-array"},
        )
        for index, native in enumerate(malformed):
            with self.subTest(malformed=index), self.assertRaises(ValueError):
                canonical.analyze(native, self.pcm / "absent", "120-15")

        malformed_case = production_manifest()
        malformed_case["cases"][0] = None
        with self.assertRaises(ValueError):
            canonical.analyze(malformed_case, self.pcm / "absent", "120-15")


if __name__ == "__main__":
    unittest.main()
