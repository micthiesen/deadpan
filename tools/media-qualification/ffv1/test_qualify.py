import copy
import unittest

import qualify


def fixture() -> tuple[dict, dict]:
    expected = {
        "width": 32,
        "height": 16,
        "frames": 2,
        "time_base": [1, 30000],
        "frame_rate": [30000, 1001],
        "pts": [0, 1001],
        "durations": [1001, 1001],
    }
    value = {
        "status": "passed",
        "source": {
            "width": 32,
            "height": 16,
            "frames": 2,
            "time_base": [1, 30000],
            "frame_rate": [30000, 1001],
            "start_pts": 0,
            "native_pts": [0, 1001],
            "durations": [1001, 1001],
        },
        "output": {
            "codec": "ffv1",
            "pixel_format": "bgr0",
            "ffv1_version": 3,
            "slice_crc": True,
            "color_range": "full",
            "color_space": "gbr",
            "color_transfer": "sRGB",
            "color_primaries": "bt709",
            "frames": 2,
            "time_base": [1, 1000],
            "frame_rate": [30000, 1001],
            "native_pts": [0, 33],
            "durations": [33, 33],
        },
    }
    return value, expected


class JsonContractTests(unittest.TestCase):
    def test_single_object_is_accepted(self) -> None:
        self.assertEqual(qualify.parse_json_line('{"status":"passed"}'), {"status": "passed"})

    def test_extra_output_duplicate_and_nonfinite_values_are_rejected(self) -> None:
        for text in (
            '{}\n{}',
            '{"status":1,"status":2}',
            '{"value":NaN}',
            '[]',
        ):
            with self.subTest(text=text), self.assertRaises(ValueError):
                qualify.parse_json_line(text)


class ResultValidationTests(unittest.TestCase):
    def test_exact_source_and_rounded_container_clock_are_accepted(self) -> None:
        value, expected = fixture()
        qualify.validate_probe_result(value, expected)

    def test_source_clock_pts_durations_and_output_contract_are_exact(self) -> None:
        value, expected = fixture()
        mutations = (
            ("source", "time_base", [1, 1000]),
            ("source", "native_pts", [0, 1000]),
            ("source", "durations", [1001, 1000]),
            ("output", "ffv1_version", 1),
            ("output", "slice_crc", False),
            ("output", "time_base", [1, 30000]),
            ("output", "native_pts", [0, 0]),
            ("output", "native_pts", [0, 34]),
            ("output", "durations", [32, 32]),
            ("output", "durations", [33, 0]),
            ("output", "durations", [33, -1]),
        )
        for section, field, replacement in mutations:
            corrupted = copy.deepcopy(value)
            corrupted[section][field] = replacement
            with self.subTest(section=section, field=field), self.assertRaises(AssertionError):
                qualify.validate_probe_result(corrupted, expected)

    def test_bool_is_not_accepted_as_a_timestamp(self) -> None:
        value, expected = fixture()
        value["output"]["native_pts"] = [False, 33]
        with self.assertRaises(AssertionError):
            qualify.validate_probe_result(value, expected)
        value, expected = fixture()
        value["source"]["time_base"] = [True, 30000]
        with self.assertRaises(AssertionError):
            qualify.validate_probe_result(value, expected)


if __name__ == "__main__":
    unittest.main()
