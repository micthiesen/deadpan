from io import BytesIO
import hashlib
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).resolve().parents[1] / "worker_protocol.py"
SPEC = importlib.util.spec_from_file_location("deadpan_worker_protocol_v2", MODULE_PATH)
worker_protocol = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = worker_protocol
SPEC.loader.exec_module(worker_protocol)

WORKER_PATH = Path(__file__).resolve().parents[1] / "worker.py"
WORKER_SPEC = importlib.util.spec_from_file_location("deadpan_worker_v2", WORKER_PATH)
worker = importlib.util.module_from_spec(WORKER_SPEC)
assert WORKER_SPEC.loader is not None
sys.modules["worker_protocol"] = worker_protocol
sys.modules[WORKER_SPEC.name] = worker
WORKER_SPEC.loader.exec_module(worker)


SHA_A = "a" * 64
SHA_B = "b" * 64


def plan():
    return {
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
            "requested_boundary_duration": {"numerator": "23023", "denominator": "15000"},
            "actual_boundary_duration": {"numerator": "5", "denominator": "3"},
            "retime_deviation": {"numerator": "659", "denominator": "5000"},
        },
        "sampling": {"endpoint_policy": "interior_only"},
    }


def bridge_request_wire():
    return {
        "operation": "generate_bridge",
        "protocol": 2,
        "identity": {"request_id": "job-17", "attempt_id": "attempt-2"},
        "cancellation_token": "cancel-17-attempt-2",
        "project_id": "project-1",
        "revision_id": "revision-9",
        "target": {"hold_id": "hold-4", "request_version": 3},
        "input": {"manifest": "inputs/context.json", "sha256": SHA_A},
        "output_workspace": "outputs",
        "constraints": {
            "video": {
                "frames": 45,
                "frame_rate": {"numerator": 30_000, "denominator": 1_001},
                "width": 768,
                "height": 320,
            },
            "conditioning": "bridge",
            "motion": "still",
        },
        "provider": {
            "pack_id": "ltx-2.3-q4-development",
            "pack_version": "56a5866d",
            "runtime_id": "ltx-mlx-development",
            "runtime_version": "0.15.8+deadpan3",
            "seed": 38_117,
        },
        "plan": plan(),
    }


def encode(value):
    output = BytesIO()
    worker_protocol.write_frame(output, value)
    return output.getvalue()


def native_candidate_wire(protocol=2):
    return {
        "event": "completed_bridge",
        "protocol": protocol,
        "identity": {"request_id": "job-17", "attempt_id": "attempt-2"},
        "candidate": {
            "native": {
                "reference": "outputs/native.mp4",
                "sha256": SHA_B,
                "byte_length": 91_337,
            },
            "provenance": {
                "reference": "outputs/provenance.json",
                "sha256": SHA_A,
                "byte_length": 12_345,
            },
            "video": {
                "frames": 41,
                "frame_rate": {"numerator": 24, "denominator": 1},
                "width": 768,
                "height": 320,
            },
            "provider": bridge_request_wire()["provider"],
        },
    }


class BridgeProtocolTests(unittest.TestCase):
    def test_all_motion_levels_and_optional_guidance_round_trip(self):
        for motion in ("still", "subtle", "moderate"):
            for instructions in (None, "Keep the hands still.", "目線をそのまま保つ。", "a" * 512,
                                 "界" * 170, 'Keep "$(code)" and /tmp/example as text.'):
                with self.subTest(motion=motion, instructions=instructions):
                    wire = bridge_request_wire()
                    wire["constraints"]["motion"] = motion
                    if instructions is not None:
                        wire["constraints"]["instructions"] = instructions
                    request = worker_protocol.parse_host_message(wire)
                    self.assertEqual(request.constraints.instructions, instructions)
                    self.assertEqual(request.to_wire(), wire)

        wire = bridge_request_wire()
        wire["constraints"]["instructions"] = None
        self.assertNotIn("instructions", worker_protocol.parse_host_message(wire).to_wire()["constraints"])

    def test_guidance_is_bounded_utf8_without_control_characters(self):
        for instructions in ("", "   ", "\u2003", "a" * 513, "界" * 171, "\ud800", 42, [],
                             "a\0b", "a\tb", "a\nb", "a\x7fb", "a\x85b", "a\x9fb"):
            with self.subTest(instructions=instructions):
                wire = bridge_request_wire()
                wire["constraints"]["instructions"] = instructions
                with self.assertRaises(worker_protocol.ProtocolError):
                    worker_protocol.parse_host_message(wire)

    def test_optional_guidance_does_not_open_unknown_fields_or_invalid_motion(self):
        for field, value in (("unknown", "text"), ("motion", "fast")):
            wire = bridge_request_wire()
            wire["constraints"]["instructions"] = "Keep the head still."
            wire["constraints"][field] = value
            with self.assertRaises(worker_protocol.ProtocolError):
                worker_protocol.parse_host_message(wire)

    def test_generate_bridge_round_trips_plan_and_uses_protocol_two(self):
        message = worker_protocol.parse_host_message(bridge_request_wire())
        self.assertIsInstance(message, worker_protocol.GenerateBridgeRequest)
        assert isinstance(message, worker_protocol.GenerateBridgeRequest)
        self.assertEqual(message.protocol, 2)
        self.assertEqual(message.plan, plan())
        self.assertEqual(message.to_wire(), bridge_request_wire())

    def test_generation_operation_and_completion_versions_cannot_cross(self):
        legacy = bridge_request_wire()
        legacy["operation"] = "generate_hold"
        legacy.pop("plan")
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.parse_host_message(legacy)

        modern = bridge_request_wire()
        modern["protocol"] = 1
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.parse_host_message(modern)

        completed = native_candidate_wire()
        completed["event"] = "completed"
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.parse_worker_message(completed)

        completed = native_candidate_wire(protocol=1)
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.parse_worker_message(completed)

    def test_modern_endpoint_requires_matching_cancel_and_emits_protocol_two(self):
        cancel = {
            "operation": "cancel",
            "protocol": 2,
            "identity": {"request_id": "job-17", "attempt_id": "attempt-2"},
            "cancellation_token": "cancel-17-attempt-2",
        }
        output = BytesIO()
        endpoint = worker_protocol.WorkerProtocol(
            BytesIO(encode(bridge_request_wire()) + encode(cancel)), output
        )
        request = endpoint.read_request()
        self.assertIsInstance(request, worker_protocol.GenerateBridgeRequest)
        self.assertTrue(endpoint.read_cancel())
        endpoint.emit_stage("preflight")
        endpoint.emit_completed_bridge(
            worker_protocol.NativeCandidateManifest(
                worker_protocol.WorkspaceArtifact("outputs/native.mp4", SHA_B, 91_337),
                worker_protocol.WorkspaceArtifact("outputs/provenance.json", SHA_A, 12_345),
                worker_protocol.VideoSpec(41, worker_protocol.FrameRate(24, 1), 768, 320),
                worker_protocol.ProviderSelection(
                    "ltx-2.3-q4-development",
                    "56a5866d",
                    "ltx-mlx-development",
                    "0.15.8+deadpan3",
                    38_117,
                ),
            )
        )
        events = []
        stream = BytesIO(output.getvalue())
        while event := worker_protocol.read_worker_message(stream):
            events.append(event)
        self.assertEqual(
            [type(event) for event in events],
            [worker_protocol.StageEvent, worker_protocol.CompletedBridgeEvent],
        )
        self.assertTrue(all(event.protocol == 2 for event in events))

        cancel["protocol"] = 1
        endpoint = worker_protocol.WorkerProtocol(
            BytesIO(encode(bridge_request_wire()) + encode(cancel)), BytesIO()
        )
        endpoint.read_request()
        with self.assertRaises(worker_protocol.ProtocolError):
            endpoint.read_cancel()

    def test_native_manifest_is_strict_and_rejects_unsafe_references(self):
        value = native_candidate_wire()
        parsed = worker_protocol.parse_worker_message(value)
        self.assertIsInstance(parsed, worker_protocol.CompletedBridgeEvent)
        assert isinstance(parsed, worker_protocol.CompletedBridgeEvent)
        self.assertEqual(parsed.candidate.native.reference, "outputs/native.mp4")

        for mutate in [
            lambda candidate: candidate.pop("provenance"),
            lambda candidate: candidate.update(extra=True),
            lambda candidate: candidate["native"].update(reference="../native.mp4"),
            lambda candidate: candidate["provenance"].update(byte_length=0),
            lambda candidate: candidate["provenance"].update(reference="outputs/native.mp4"),
        ]:
            value = native_candidate_wire()
            mutate(value["candidate"])
            with self.subTest(mutation=mutate):
                with self.assertRaises(worker_protocol.ProtocolError):
                    worker_protocol.parse_worker_message(value)

    def test_bridge_plan_schema_rejects_unknown_wrong_and_inconsistent_fields(self):
        mutations = [
            lambda value: value["plan"].update(extra=True),
            lambda value: value["plan"].update(schema_version=True),
            lambda value: value["plan"]["project"].update(interior_frames=True),
            lambda value: value["plan"]["native"].update(frame_count=-1),
            lambda value: value["plan"]["native"]["frame_rate"].update(numerator=24.0),
            lambda value: value["plan"]["timing"].update(
                actual_boundary_duration={"numerator": "1", "denominator": "0"}
            ),
            lambda value: value["plan"]["timing"].update(
                retime_deviation={"numerator": "1_0", "denominator": "1"}
            ),
            lambda value: value["plan"]["sampling"].update(endpoint_policy="include_endpoints"),
            lambda value: value["plan"]["timing"].update(
                retime_deviation={"numerator": "0", "denominator": "1"}
            ),
        ]
        for mutate in mutations:
            value = bridge_request_wire()
            mutate(value)
            with self.subTest(mutation=mutate):
                with self.assertRaises(worker_protocol.ProtocolError):
                    worker_protocol.parse_host_message(value)


class WorkerContextShapeTests(unittest.TestCase):
    def context(self):
        artifact = {"reference": "inputs/left.png", "sha256": SHA_A, "byte_length": 1}
        return {
            "schema_version": 2,
            "model_color_space": {"transfer": "srgb", "primaries": "bt709",
                                  "matrix": "rgb", "range": "full"},
            "plan": plan(), "left": artifact, "right": dict(artifact, reference="inputs/right.png"),
            "input_color_interpretation": "decoded rgb8 as srgb",
            "boundaries": {"left": {"authored_black": {"project_frame": 0}},
                           "right": {"authored_black": {"project_frame": 46}}},
        }

    def schema1_context(self):
        context = self.context()
        del context["model_color_space"], context["boundaries"]
        context.update(schema_version=1, model_color="srgb")
        return context

    def test_measured_and_legacy_contexts_are_admitted(self):
        worker.validate_context_shape(self.context())
        worker.validate_context_shape(self.schema1_context())

    def schema3_context(self):
        context = self.context()
        context.update(
            schema_version=3,
            geometry={
                "presentation": {"x": 4, "y": 10, "width": 760, "height": 300},
                "left_content": {"x": 64, "y": 40, "width": 640, "height": 240},
                "right_content": None,
            },
        )
        context["boundaries"]["left"] = {
            "original": {
                "project_frame": 0,
                "asset": "asset",
                "qualification": "a" * 64,
                "picture": {
                    "source_frame": 0,
                    "pts": {"ticks": 0, "time_base": {"numerator": 1, "denominator": 24}},
                    "stream": {
                        "codec": "h264", "pixel_format": "yuv420p", "width": 768, "height": 320,
                        "sample_aspect": [1, 1], "rotation_quarter_turns": 0,
                        "decoded_sample_bits": 8,
                        "color": {"transfer": "bt709", "primaries": "bt709",
                                  "matrix": "bt709", "range": "limited"},
                    },
                    "model_input": "rec709_to_srgb",
                },
            }
        }
        return context

    def test_captured_geometry_schema_three_is_admitted(self):
        worker.validate_context_shape(self.schema3_context())

    def test_schema_three_geometry_is_strict_and_raster_bound(self):
        mutations = [
            lambda value: value.pop("geometry"),
            lambda value: value["geometry"].update(extra=True),
            lambda value: value["geometry"]["presentation"].update(extra=True),
            lambda value: value["geometry"]["presentation"].update(x=True),
            lambda value: value["geometry"]["presentation"].update(width=0),
            lambda value: value["geometry"]["presentation"].update(x=(1 << 32) - 1, width=2),
            lambda value: value["geometry"]["presentation"].update(width=769),
            lambda value: value["geometry"]["presentation"].update(x=5),
            lambda value: value["geometry"]["left_content"].update(y=41),
            lambda value: value["geometry"]["left_content"].update(width=767),
            lambda value: value["geometry"].update(right_content={"x": 64, "y": 40,
                                                                   "width": 640, "height": 240}),
            lambda value: value["geometry"].update(left_content=None),
            lambda value: value["boundaries"]["left"]["original"].update(extra=True),
            lambda value: value["boundaries"]["left"]["original"].pop("picture"),
        ]
        for mutate in mutations:
            value = self.schema3_context()
            mutate(value)
            with self.subTest(mutation=mutate):
                with self.assertRaises(ValueError):
                    worker.validate_context_shape(value)

    def test_bridge_validation_rechecks_geometry_against_the_request(self):
        request = worker_protocol.parse_host_message(bridge_request_wire())
        context = self.schema3_context()
        video = bridge_request_wire()["constraints"]["video"]
        self.assertEqual(worker.validate_bridge_context(context, request, video), (45, 41))
        context["geometry"]["presentation"]["x"] = 5
        with self.assertRaisesRegex(ValueError, "not centered"):
            worker.validate_bridge_context(context, request, video)

    def test_input_color_interpretation_matches_rust_utf8_and_trim_rules(self):
        factories = (self.schema1_context, self.context, self.schema3_context)
        for make_context in factories:
            for value in (" \t\n\u0085\u3000", "opaque\0interpretation", "界" * 1366):
                with self.subTest(schema=make_context().get("schema_version"), value=value[:16]):
                    context = make_context()
                    context["input_color_interpretation"] = value
                    with self.assertRaises(ValueError):
                        worker.validate_context_shape(context)

            for value in ("é" * 2048, "\u001c"):
                with self.subTest(schema=make_context().get("schema_version"), value=value[:16]):
                    context = make_context()
                    context["input_color_interpretation"] = value
                    worker.validate_context_shape(context)

    def test_foreign_model_colour_and_unknown_shapes_are_refused(self):
        mutations = [
            lambda value: value["model_color_space"].update(primaries="bt2020"),
            lambda value: value["model_color_space"].update(gamma=2.2),
            lambda value: value.update(schema_version=4),
            lambda value: value.update(schema_version=True),
            lambda value: value.update(model_color="srgb"),
            lambda value: value.pop("boundaries"),
            lambda value: value["boundaries"].update(middle={}),
            lambda value: value.update(input_color_interpretation=""),
        ]
        for mutate in mutations:
            value = self.context()
            mutate(value)
            with self.subTest(mutation=mutate):
                with self.assertRaises(ValueError):
                    worker.validate_context_shape(value)


class WorkerFinishedOutputTests(unittest.TestCase):
    def test_request_plan_must_equal_context_plan_before_inference(self):
        request = worker_protocol.parse_host_message(bridge_request_wire())
        context = {"plan": plan()}
        self.assertEqual(
            worker.validate_bridge_context(context, request, bridge_request_wire()["constraints"]["video"]),
            (45, 41),
        )
        context["plan"]["native"]["frame_count"] = 49
        with self.assertRaisesRegex(ValueError, "plan differs"):
            worker.validate_bridge_context(
                context, request, bridge_request_wire()["constraints"]["video"]
            )

    def test_finished_native_and_provenance_artifacts_use_actual_hash_and_length(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            native = output / "native.mp4"
            provenance = output / "provenance.json"
            native_bytes = b"native-media"
            provenance_bytes = b'{"schema_version":2,"request_binding":{}}\n'
            native.write_bytes(native_bytes)
            provenance.write_bytes(provenance_bytes)

            native_artifact = worker.finished_artifact(
                output, native, "outputs/native.mp4", 1024, lambda: None
            )
            provenance_artifact = worker.finished_artifact(
                output, provenance, "outputs/provenance.json", 1024, lambda: None
            )
            self.assertEqual(native_artifact.byte_length, len(native_bytes))
            self.assertEqual(
                native_artifact.sha256, hashlib.sha256(native_bytes).hexdigest()
            )
            self.assertEqual(
                provenance_artifact.sha256, hashlib.sha256(provenance_bytes).hexdigest()
            )

            with self.assertRaisesRegex(ValueError, "unexpected output reference"):
                worker.finished_artifact(output, native, "outputs/other.mp4", 1024, lambda: None)
            with self.assertRaisesRegex(ValueError, "bounded regular file"):
                worker.finished_artifact(output, native, "outputs/native.mp4", 1, lambda: None)


if __name__ == "__main__":
    unittest.main()
