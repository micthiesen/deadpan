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
            "runtime_version": "0.15.8+deadpan1",
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
                    "0.15.8+deadpan1",
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
