import copy
from fractions import Fraction
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch


MODULE_PATH = Path(__file__).resolve().parents[1] / "worker_media.py"
SPEC = importlib.util.spec_from_file_location("deadpan_worker_media", MODULE_PATH)
worker_media = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(worker_media)


def ratio_wire(value):
    value = Fraction(value)
    return {"numerator": str(value.numerator), "denominator": str(value.denominator)}


def plan_for(
    count=45,
    project_numerator=30_000,
    project_denominator=1_001,
    model_count=41,
    native_numerator=24,
    native_denominator=1,
    width=768,
    height=320,
):
    project_rate = Fraction(project_numerator, project_denominator)
    native_rate = Fraction(native_numerator, native_denominator)
    requested = Fraction(count + 1, 1) / project_rate
    actual = Fraction(model_count - 1, 1) / native_rate
    return {
        "schema_version": 1,
        "operation": "bridge",
        "interpolation": "linear",
        "project": {
            "interior_frames": count,
            "frame_rate": {
                "numerator": project_numerator,
                "denominator": project_denominator,
            },
        },
        "native": {
            "frame_count": model_count,
            "frame_rate": {
                "numerator": native_numerator,
                "denominator": native_denominator,
            },
            "width": width,
            "height": height,
        },
        "timing": {
            "requested_boundary_duration": ratio_wire(requested),
            "actual_boundary_duration": ratio_wire(actual),
            "retime_deviation": ratio_wire(actual - requested),
        },
        "sampling": {"endpoint_policy": "interior_only"},
    }


def video_for(plan):
    return {
        "frames": plan["project"]["interior_frames"],
        "frame_rate": copy.deepcopy(plan["project"]["frame_rate"]),
        "width": plan["native"]["width"],
        "height": plan["native"]["height"],
    }


class SamplePositionTests(unittest.TestCase):
    def test_fractional_positions_are_exact_and_exclude_endpoints(self):
        positions = list(worker_media.sample_positions(4, 9))
        self.assertEqual(
            positions,
            [
                (1, 2, 3, 5),
                (3, 4, 1, 5),
                (4, 5, 4, 5),
                (6, 7, 2, 5),
            ],
        )
        self.assertEqual(len(positions), 4)
        for lower, upper, numerator, denominator in positions:
            position = Fraction(lower * denominator + numerator, denominator)
            self.assertGreater(position, 0)
            self.assertLess(position, 8)
            self.assertIn(upper - lower, (0, 1))

    def test_integer_positions_and_one_frame_midpoint(self):
        self.assertEqual(
            list(worker_media.sample_positions(3, 5)),
            [(1, 1, 0, 4), (2, 2, 0, 4), (3, 3, 0, 4)],
        )
        self.assertEqual(list(worker_media.sample_positions(1, 9)), [(4, 4, 0, 2)])

    def test_position_limits_reject_bools_and_out_of_envelope_values(self):
        for count, model_count in [
            (True, 9),
            (0, 9),
            (181, 9),
            (1, True),
            (1, 1),
            (1, 98),
        ]:
            with self.subTest(count=count, model_count=model_count):
                with self.assertRaises(ValueError):
                    list(worker_media.sample_positions(count, model_count))


class EncodedLinearBlendTests(unittest.TestCase):
    def test_half_up_quantization_happens_after_encoded_space_interpolation(self):
        cases = [
            (0, 255, 1, 2, 128),
            (10, 11, 1, 2, 11),
            (11, 10, 1, 2, 11),
            (20, 100, 1, 4, 40),
            (20, 100, 3, 4, 80),
            (37, 201, 0, 7, 37),
            (37, 201, 7, 7, 201),
        ]
        for left, right, numerator, denominator, expected in cases:
            with self.subTest(
                left=left,
                right=right,
                numerator=numerator,
                denominator=denominator,
            ):
                self.assertEqual(
                    worker_media.blend_channel(left, right, numerator, denominator),
                    expected,
                )


class ValidatePlanTests(unittest.TestCase):
    def test_rust_schema_golden_shape_and_fractional_rate(self):
        plan = plan_for()
        self.assertEqual(
            plan,
            {
                "schema_version": 1,
                "operation": "bridge",
                "interpolation": "linear",
                "project": {
                    "interior_frames": 45,
                    "frame_rate": {"numerator": 30_000, "denominator": 1_001},
                },
                "native": {
                    "frame_count": 41,
                    "frame_rate": {"numerator": 24, "denominator": 1},
                    "width": 768,
                    "height": 320,
                },
                "timing": {
                    "requested_boundary_duration": {
                        "numerator": "23023",
                        "denominator": "15000",
                    },
                    "actual_boundary_duration": {
                        "numerator": "5",
                        "denominator": "3",
                    },
                    "retime_deviation": {
                        "numerator": "659",
                        "denominator": "5000",
                    },
                },
                "sampling": {"endpoint_policy": "interior_only"},
            },
        )
        self.assertEqual(worker_media.validate_plan(plan, video_for(plan)), (45, 41))

    def test_exact_rate_and_zero_deviation(self):
        plan = plan_for(
            count=23,
            project_numerator=24,
            project_denominator=1,
            model_count=25,
        )
        self.assertEqual(
            plan["timing"]["retime_deviation"],
            {"numerator": "0", "denominator": "1"},
        )
        self.assertEqual(worker_media.validate_plan(plan, video_for(plan)), (23, 25))

    def test_runtime_envelope_accepts_its_frame_extremes(self):
        minimum = plan_for(
            count=1,
            project_numerator=1,
            project_denominator=1,
            model_count=49,
        )
        maximum = plan_for(
            count=180,
            project_numerator=120,
            project_denominator=1,
            model_count=41,
        )
        self.assertEqual(worker_media.validate_plan(minimum, video_for(minimum)), (1, 49))
        self.assertEqual(worker_media.validate_plan(maximum, video_for(maximum)), (180, 41))

    def test_unknown_fields_are_rejected_at_every_object_boundary(self):
        mutations = [
            lambda plan: plan.update(extra=True),
            lambda plan: plan["project"].update(extra=True),
            lambda plan: plan["project"]["frame_rate"].update(extra=True),
            lambda plan: plan["native"].update(extra=True),
            lambda plan: plan["native"]["frame_rate"].update(extra=True),
            lambda plan: plan["timing"].update(extra=True),
            lambda plan: plan["timing"]["actual_boundary_duration"].update(extra=True),
            lambda plan: plan["sampling"].update(extra=True),
        ]
        for mutate in mutations:
            plan = plan_for()
            mutate(plan)
            with self.subTest(mutation=mutate):
                with self.assertRaises(ValueError):
                    worker_media.validate_plan(plan, video_for(plan))

    def test_bool_values_do_not_pass_as_integers(self):
        paths = [
            ("schema_version",),
            ("project", "interior_frames"),
            ("project", "frame_rate", "numerator"),
            ("project", "frame_rate", "denominator"),
            ("native", "frame_count"),
            ("native", "frame_rate", "numerator"),
            ("native", "frame_rate", "denominator"),
        ]
        for path in paths:
            plan = plan_for()
            target = plan
            for component in path[:-1]:
                target = target[component]
            target[path[-1]] = True
            with self.subTest(path=path):
                with self.assertRaises(ValueError):
                    worker_media.validate_plan(plan, video_for(plan))

    def test_invalid_envelope_and_nearest_count_values_are_rejected(self):
        cases = [
            plan_for(count=0, model_count=9),
            plan_for(count=181, project_numerator=120, model_count=41),
            plan_for(project_numerator=121, model_count=17),
            plan_for(project_numerator=1, project_denominator=2, model_count=97),
            plan_for(native_numerator=25, model_count=41),
            plan_for(width=512, model_count=41),
            plan_for(height=384, model_count=41),
            plan_for(model_count=33),
            plan_for(count=180, project_numerator=1, model_count=97),
        ]
        for plan in cases:
            with self.subTest(plan=plan):
                with self.assertRaises(ValueError):
                    worker_media.validate_plan(plan, video_for(plan))

    def test_corrupted_plan_contract_and_timing_are_rejected(self):
        mutations = [
            lambda plan: plan.update(schema_version=2),
            lambda plan: plan.update(operation="extend_from_left"),
            lambda plan: plan.update(interpolation="nearest"),
            lambda plan: plan.update(sampling={"endpoint_policy": "include_endpoints"}),
            lambda plan: plan["timing"].update(
                requested_boundary_duration={"numerator": "1", "denominator": "1"}
            ),
            lambda plan: plan["timing"].update(
                actual_boundary_duration={"numerator": "1", "denominator": "1"}
            ),
            lambda plan: plan["timing"].update(
                retime_deviation={"numerator": "1", "denominator": "1"}
            ),
            lambda plan: plan["timing"].update(
                retime_deviation={"numerator": 1, "denominator": "1"}
            ),
        ]
        for mutate in mutations:
            plan = plan_for()
            mutate(plan)
            with self.subTest(mutation=mutate):
                with self.assertRaises(ValueError):
                    worker_media.validate_plan(plan, video_for(plan))

    def test_exact_ratio_denominators_must_be_positive(self):
        for denominator, numerator in [("0", "23023"), ("-15000", "-23023")]:
            plan = plan_for()
            plan["timing"]["requested_boundary_duration"] = {
                "numerator": numerator,
                "denominator": denominator,
            }
            with self.subTest(denominator=denominator):
                with self.assertRaises(ValueError):
                    worker_media.validate_plan(plan, video_for(plan))

    def test_authored_video_must_match_plan_exactly(self):
        plan = plan_for()
        videos = [
            {**video_for(plan), "frames": 44},
            {**video_for(plan), "width": 512},
            {**video_for(plan), "extra": True},
            {**video_for(plan), "frame_rate": {"numerator": 30, "denominator": 1}},
        ]
        for video in videos:
            with self.subTest(video=video):
                with self.assertRaises(ValueError):
                    worker_media.validate_plan(plan, video)

    def test_authored_video_bools_do_not_equal_integer_fields(self):
        plan = plan_for(
            count=1,
            project_numerator=1,
            project_denominator=1,
            model_count=49,
        )
        video = video_for(plan)
        video["frames"] = True
        video["frame_rate"] = {"numerator": True, "denominator": True}
        with self.assertRaises(ValueError):
            worker_media.validate_plan(plan, video)


class EncodedTimingTests(unittest.TestCase):
    def test_average_rate_and_total_duration_cannot_hide_wrong_frame_times(self):
        stream = dict(worker_media.COLOR, codec_type="video", width=768, height=320,
                      r_frame_rate="24/1", avg_frame_rate="24/1", start_pts=0,
                      duration_ts=2, time_base="1/24")
        for frames in [
            [{"best_effort_timestamp": 0, "duration": 1},
             {"best_effort_timestamp": 2, "duration": 1}],
            [{"best_effort_timestamp": 0, "duration": 2},
             {"best_effort_timestamp": 1, "duration": 1}],
            [{"best_effort_timestamp": 0, "duration": 1}],
        ]:
            result = SimpleNamespace(stdout=json.dumps({"streams": [stream], "frames": frames}).encode())
            with self.subTest(frames=frames), patch.object(worker_media.subprocess, "run", return_value=result), \
                    patch.object(worker_media.subprocess, "Popen") as decoder:
                with self.assertRaises(ValueError):
                    worker_media.verify_rgb("ffmpeg", "ffprobe", "snapshot.mp4",
                                            {"frames": 2, "rgb_sha256": "unused"}, 768, 320,
                                            {"numerator": 24, "denominator": 1}, lambda: None)
                decoder.assert_not_called()


if __name__ == "__main__":
    unittest.main()
