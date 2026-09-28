"""Full-plane and admission oracles without GPU, decoder or compiler execution."""

import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from qualify_picture_export import COLOR, PictureHarness, check_decode, compare_planes, renderer_cases


class PictureExportTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)

    def fixture(self, width):
        name = f"linear-rec709-patches-chroma-{width}x180"
        count = width * 180 * 3 // 2
        actual, reference = bytes([101]) * count, bytes([100]) * count
        paths = [self.root / (name + suffix) for suffix in (".i420", "-reference.i420")]
        for path, content in zip(paths, (actual, reference)):
            path.write_bytes(content)
        return {"name": name, "width": width, "height": 180, "raw_path": str(paths[0]),
                "reference_path": str(paths[1]), "actual_sha256": hashlib.sha256(actual).hexdigest(),
                "reference_sha256": hashlib.sha256(reference).hexdigest(), "pixel_format": "yuv420p",
                "plane_order": ["Y", "Cb", "Cr"], "frame_rate": [30, 1], "frame_count": 1,
                "byte_count": count, "y_stride_bytes": width, "chroma_stride_bytes": width // 2,
                "color": COLOR.copy(), "comparison": {"passed": True}}

    def report(self):
        return {"schema_version": 1, "status": "passed", "adapter": {"backend": "Metal"},
                "case_count": 2, "checks": [{"passed": True}], "fixture_directory": str(self.root),
                "cases": [self.fixture(320), self.fixture(318)]}

    @staticmethod
    def decoded(width=320):
        return {"schema_version": 1, "kind": "decode_plane", "decoder_drained": True, "frame_count": 1, "audio_streams": 0,
                "profile": "High", "time_base": [1, 15360], "stream_start_pts": 0, "stream_duration": 512,
                "decoded_bytes": width * 180 * 3 // 2, "stream_sample_aspect_ratio": [1, 1],
                "frames": [{"width": width, "height": 180, "pixel_format": "yuv420p", "bit_depth": 8,
                            "chroma_subsampling": [2, 2], "interlaced": False, "decode_error_flags": 0, "flags": 2,
                            "color_range": "tv", "color_space": "bt709", "color_transfer": "bt709",
                            "color_primaries": "bt709", "chroma_location": "left", "sample_aspect_ratio": [0, 1],
                            "pts": 0, "best_effort_pts": 0, "duration": 512}]}

    def test_full_plane_comparison_reports_last_pixel_and_chroma_coordinates(self):
        expected = bytes([100]) * (318 * 180 * 3 // 2)
        actual = bytearray(expected)
        actual[-1] = 140
        report = compare_planes(actual, expected, 318, 180)
        self.assertFalse(report["passed"])
        self.assertEqual(report["compared_codes"], len(expected))
        failure = report["planes"][2]["first_failures"][0]
        self.assertEqual((failure["x"], failure["y"], failure["byte_offset"]), (158, 89, len(expected) - 1))

    def test_mean_error_is_independent_of_peak_tolerance(self):
        expected = bytes([100]) * (320 * 180 * 3 // 2)
        report = compare_planes(bytes([103]) * len(expected), expected, 320, 180)
        self.assertFalse(report["passed"])
        self.assertTrue(all(plane["out_of_tolerance_codes"] == 0 for plane in report["planes"]))

    def test_swapped_chroma_and_padded_rows_cannot_pass(self):
        luma = 318 * 180
        expected = bytes([100]) * luma + bytes([30]) * (luma // 4) + bytes([220]) * (luma // 4)
        swapped = expected[:luma] + expected[luma + luma // 4:] + expected[luma:luma + luma // 4]
        self.assertFalse(compare_planes(swapped, expected, 318, 180)["passed"])
        with self.assertRaises(ValueError):
            compare_planes(expected + bytes(180), expected, 318, 180)

    def test_renderer_report_requires_real_success_exact_pair_and_hashes(self):
        report = self.report()
        self.assertEqual(len(renderer_cases(report)), 2)
        for field, replacement in (("status", "failed"), ("adapter", {"backend": "Vulkan"}), ("checks", [])):
            changed = copy.deepcopy(report)
            changed[field] = replacement
            with self.subTest(field=field), self.assertRaises(ValueError):
                renderer_cases(changed)
        report["cases"][0]["actual_sha256"] = "missing"
        with self.assertRaises(ValueError):
            renderer_cases(report)

    def test_metadata_and_timing_changes_fail_without_touching_pixels(self):
        original = self.decoded()
        self.assertTrue(all(check["passed"] for check in check_decode(original, 320, 180)))
        for field, value in (("color_transfer", "iec61966-2-1"), ("chroma_location", "center"),
                             ("duration", 511), ("bit_depth", 10), ("interlaced", True), ("flags", 3)):
            changed = copy.deepcopy(original)
            changed["frames"][0][field] = value
            with self.subTest(field=field):
                self.assertFalse(all(check["passed"] for check in check_decode(changed, 320, 180)))

    def test_sar_fallback_requires_valid_unknown_frame_ratio_and_explicit_stream_ratio(self):
        for frame_sar, stream_sar, expected in (
            ([0, 1], [1, 1], True), ([0, 5], [1, 1], True), ([2, 2], [4, 3], True),
            ([0, 0], [1, 1], False), ([True, 1], [1, 1], False), ([0, True], [1, 1], False),
            ([0, 1], [True, 1], False), ([0, 1], [1, 0], False), ([0, 1], [0, 1], False),
            ([0, 1], [4, 3], False), (None, [1, 1], False), ([1], [1, 1], False),
        ):
            with self.subTest(frame=frame_sar, stream=stream_sar):
                report = self.decoded()
                report["frames"][0]["sample_aspect_ratio"] = frame_sar
                report["stream_sample_aspect_ratio"] = stream_sar
                checks = check_decode(report, 320, 180)
                sar_check = next(check for check in checks if check["label"] == "explicit square pixels")
                self.assertEqual(sar_check["passed"], expected)
                self.assertEqual(len(checks), 7)
                self.assertTrue(checks[-1]["passed"])

    def test_runner_encodes_actual_not_reference_and_compares_actual(self):
        fixture = self.fixture(320)
        build = self.root / "build.json"
        build.write_text(json.dumps({"prefix": str(self.root / "prefix")}))
        harness = PictureHarness(self.root, False, build, self.root / "renderer.json")
        calls = []
        def run(argv, **kwargs):
            calls.append(argv)
            if argv[1] == "encode-plane":
                self.assertEqual(argv[2], fixture["raw_path"])
                self.assertNotEqual(argv[2], fixture["reference_path"])
                observation = {"schema_version": 1, "kind": "encode_plane", "input_sha256": fixture["actual_sha256"], "encoder_drained": True,
                               "frame_count": 1, "packet_count": 1, "audio_streams": 0,
                               "input_bytes": fixture["byte_count"]}
            else:
                Path(argv[3]).write_bytes(Path(fixture["raw_path"]).read_bytes())
                observation = self.decoded()
            return subprocess.CompletedProcess(argv, 0, json.dumps(observation), "")
        with mock.patch.object(harness, "run", side_effect=run), \
             mock.patch("qualify_picture_export.inspect_mp4", return_value={"has_edts": False, "has_elst": False,
                        "fast_start": {"moov_before_mdat": True}}):
            case = harness.capture_planes(fixture)
        self.assertEqual(case["status"], "passed")
        self.assertEqual(len(calls), 2)
        self.assertTrue(all(plane["maximum_error"] == 0 for plane in case["decoded_vs_actual"]["planes"]))
        self.assertTrue(all(plane["maximum_error"] == 1 for plane in case["renderer_reference_comparison"]["planes"]))

    def test_changed_actual_bytes_fail_before_encoding(self):
        fixture = self.fixture(318)
        Path(fixture["raw_path"]).write_bytes(bytes(fixture["byte_count"]))
        build = self.root / "build.json"
        build.write_text(json.dumps({"prefix": str(self.root / "prefix")}))
        harness = PictureHarness(self.root, False, build, self.root / "renderer.json")
        with mock.patch.object(harness, "run") as run:
            case = harness.capture_planes(fixture)
        run.assert_not_called()
        self.assertEqual(case["status"], "failed")
        self.assertIn("SHA-256 mismatch", case["failure"])


if __name__ == "__main__":
    unittest.main()
