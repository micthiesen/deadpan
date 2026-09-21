from io import BytesIO
import importlib.util
from pathlib import Path
import struct
import sys
import unittest


MODULE_PATH = Path(__file__).resolve().parents[1] / "worker_protocol.py"
SPEC = importlib.util.spec_from_file_location("deadpan_worker_protocol", MODULE_PATH)
worker_protocol = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = worker_protocol
SPEC.loader.exec_module(worker_protocol)


SHA_A = "a" * 64
SHA_B = "b" * 64


def request_wire():
    return {
        "operation": "generate_hold",
        "protocol": 1,
        "identity": {"request_id": "job-17", "attempt_id": "attempt-2"},
        "cancellation_token": "cancel-17-attempt-2",
        "project_id": "project-1",
        "revision_id": "revision-9",
        "target": {"hold_id": "hold-4", "request_version": 3},
        "input": {"manifest": "inputs/context.json", "sha256": SHA_A},
        "output_workspace": "work/job-17/attempt-2",
        "constraints": {
            "video": {
                "frames": 45,
                "frame_rate": {"numerator": 30_000, "denominator": 1_001},
                "width": 512,
                "height": 320,
            },
            "conditioning": "bridge",
            "motion": "subtle",
        },
        "provider": {
            "pack_id": "qualified-local-pack",
            "pack_version": "2026.09",
            "runtime_id": "mlx",
            "runtime_version": "1.2.3+deadpan",
            "seed": 38_117,
        },
    }


def encode(value):
    output = BytesIO()
    worker_protocol.write_frame(output, value)
    return output.getvalue()


def candidate():
    return worker_protocol.CandidateManifest(
        media=worker_protocol.WorkspaceArtifact("outputs/candidate.mov", SHA_B, 91_337),
        video=worker_protocol.VideoSpec(
            45, worker_protocol.FrameRate(30_000, 1_001), 512, 320
        ),
        provider=worker_protocol.ProviderSelection(
            "qualified-local-pack", "2026.09", "mlx", "1.2.3+deadpan", 38_117
        ),
    )


class FragmentedReader:
    def __init__(self, data, maximum=1):
        self.inner = BytesIO(data)
        self.maximum = maximum

    def read(self, size=-1):
        return self.inner.read(min(size, self.maximum))


class FragmentedWriter:
    def __init__(self, maximum=1):
        self.inner = BytesIO()
        self.maximum = maximum

    def write(self, data):
        data = bytes(data)
        return self.inner.write(data[: self.maximum])

    def flush(self):
        return None

    def value(self):
        return self.inner.getvalue()


class HeaderOnlyReader:
    def __init__(self, header):
        self.header = header
        self.read_sizes = []

    def read(self, size=-1):
        self.read_sizes.append(size)
        return self.header


class FramingTests(unittest.TestCase):
    def test_fragmented_rust_shaped_request_round_trips_and_normalizes_rate(self):
        reader = FragmentedReader(encode(request_wire()), maximum=1)
        message = worker_protocol.read_host_message(reader)
        self.assertIsInstance(message, worker_protocol.GenerateHoldRequest)
        assert isinstance(message, worker_protocol.GenerateHoldRequest)
        self.assertEqual(message.identity.request_id, "job-17")
        self.assertEqual(message.constraints.video.frame_rate, worker_protocol.FrameRate(30_000, 1_001))
        self.assertEqual(message.constraints.video.frame_rate.numerator, 30_000)
        self.assertIsNone(worker_protocol.read_frame(reader))

    def test_fragmented_writer_writes_header_and_body_completely(self):
        writer = FragmentedWriter(maximum=1)
        worker_protocol.write_frame(writer, {"ok": True})
        self.assertEqual(worker_protocol.read_frame(BytesIO(writer.value())), {"ok": True})

    def test_clean_eof_and_truncation_are_distinct(self):
        self.assertIsNone(worker_protocol.read_frame(BytesIO()))
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.read_frame(BytesIO(b"\x00\x00\x00"))
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.read_frame(BytesIO(struct.pack(">I", 4) + b"{}"))

    def test_oversized_header_is_rejected_before_body_read(self):
        declared = worker_protocol.MAX_FRAME_BYTES + 1
        reader = HeaderOnlyReader(struct.pack(">I", declared))
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.read_frame(reader)
        self.assertEqual(reader.read_sizes, [4])

    def test_maximum_legal_frame_survives_one_byte_fragmentation(self):
        prefix = b'{"data":"'
        suffix = b'"}'
        payload = prefix + b"x" * (
            worker_protocol.MAX_FRAME_BYTES - len(prefix) - len(suffix)
        ) + suffix
        self.assertEqual(len(payload), worker_protocol.MAX_FRAME_BYTES)
        reader = FragmentedReader(
            struct.pack(">I", len(payload)) + payload,
            maximum=1,
        )
        value = worker_protocol.read_frame(reader)
        self.assertIsInstance(value, dict)
        assert value is not None
        self.assertEqual(len(value["data"]), len(payload) - len(prefix) - len(suffix))

    def test_parser_resource_errors_are_protocol_errors(self):
        giant_integer = b'{"number":' + b"9" * 5_000 + b"}"
        deep_json = b'{"nested":' + b"[" * 120_000 + b"]" * 120_000 + b"}"
        for payload in (giant_integer, deep_json):
            with self.subTest(payload_length=len(payload)), self.assertRaises(
                worker_protocol.ProtocolError
            ):
                worker_protocol.read_frame(
                    BytesIO(struct.pack(">I", len(payload)) + payload)
                )

    def test_json_duplicate_unknown_nonfinite_and_nonobject_are_rejected(self):
        duplicate = b'{"operation":"cancel","operation":"cancel"}'
        for payload in [
            duplicate,
            b'{"operation":"cancel","protocol":NaN}',
            b"[]",
        ]:
            with self.subTest(payload=payload), self.assertRaises(worker_protocol.ProtocolError):
                worker_protocol.read_frame(BytesIO(struct.pack(">I", len(payload)) + payload))
        unknown = worker_protocol.read_frame(
            BytesIO(encode({"operation": "cancel", "extra": 1}))
        )
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.parse_host_message(unknown)

    def test_write_rejects_oversized_payload(self):
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.write_frame(
                BytesIO(), {"data": "x" * worker_protocol.MAX_FRAME_BYTES}
            )


class ValidationTests(unittest.TestCase):
    def assert_bad_request(self, mutate):
        value = request_wire()
        mutate(value)
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.parse_host_message(value)

    def test_integer_fields_reject_bool_float_and_out_of_range_values(self):
        self.assert_bad_request(lambda value: value.__setitem__("protocol", True))
        self.assert_bad_request(lambda value: value["target"].__setitem__("request_version", 1.0))
        self.assert_bad_request(lambda value: value["constraints"]["video"].__setitem__("frames", 0))
        self.assert_bad_request(
            lambda value: value["constraints"]["video"].__setitem__("width", 32_769)
        )
        self.assert_bad_request(
            lambda value: value["constraints"]["video"]["frame_rate"].__setitem__(
                "numerator", 0
            )
        )

    def test_paths_hashes_ids_and_nested_unknown_fields_are_strict(self):
        for path in ["/absolute", "../escape", "safe/../escape", "safe//file", "safe\\file"]:
            self.assert_bad_request(lambda value, path=path: value.__setitem__("output_workspace", path))
        self.assert_bad_request(lambda value: value["input"].__setitem__("sha256", "A" * 64))
        self.assert_bad_request(lambda value: value["identity"].__setitem__("extra", "x"))
        self.assert_bad_request(lambda value: value["provider"].__setitem__("extra", "x"))

    def test_progress_artifact_and_diagnostic_bounds_are_strict(self):
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.StageProgress(2, 1)
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.WorkspaceArtifact("out.bin", SHA_A, 0)
        with self.assertRaises(worker_protocol.ProtocolError):
            worker_protocol.WorkerFailure("internal", "x" * (worker_protocol.MAX_DIAGNOSTIC_BYTES + 1))


class WorkerEndpointTests(unittest.TestCase):
    def setUp(self):
        self.request = worker_protocol.parse_host_message(request_wire())
        assert isinstance(self.request, worker_protocol.GenerateHoldRequest)
        self.input = BytesIO(encode(request_wire()))
        self.output = BytesIO()
        self.endpoint = worker_protocol.WorkerProtocol(self.input, self.output)

    def test_request_then_matching_cancel_and_cancel_event(self):
        self.input = BytesIO(
            encode(request_wire())
            + encode(
                {
                    "operation": "cancel",
                    "protocol": 1,
                    "identity": {"request_id": "job-17", "attempt_id": "attempt-2"},
                    "cancellation_token": "cancel-17-attempt-2",
                }
            )
        )
        self.endpoint = worker_protocol.WorkerProtocol(self.input, self.output)
        self.endpoint.read_request()
        self.assertTrue(self.endpoint.read_cancel())
        self.assertTrue(self.endpoint.cancel_requested)
        self.endpoint.emit_cancelled()
        event = worker_protocol.read_worker_message(BytesIO(self.output.getvalue()))
        self.assertIsInstance(event, worker_protocol.CancelledEvent)
        with self.assertRaises(worker_protocol.ProtocolError):
            self.endpoint.emit_stage("inference")

    def test_wrong_identity_token_and_extra_request_are_rejected(self):
        for change in [
            lambda value: value["identity"].__setitem__("attempt_id", "other"),
            lambda value: value.__setitem__("cancellation_token", "other"),
        ]:
            with self.subTest(change=change):
                cancel = {
                    "operation": "cancel",
                    "protocol": 1,
                    "identity": {"request_id": "job-17", "attempt_id": "attempt-2"},
                    "cancellation_token": "cancel-17-attempt-2",
                }
                change(cancel)
                endpoint = worker_protocol.WorkerProtocol(
                    BytesIO(encode(request_wire()) + encode(cancel)), BytesIO()
                )
                endpoint.read_request()
                with self.assertRaises(worker_protocol.ProtocolError):
                    endpoint.read_cancel()

        endpoint = worker_protocol.WorkerProtocol(
            BytesIO(encode(request_wire()) + encode(request_wire())), BytesIO()
        )
        endpoint.read_request()
        with self.assertRaises(worker_protocol.ProtocolError):
            endpoint.read_cancel()

    def test_emit_helpers_preserve_identity_and_terminal_state(self):
        self.endpoint.read_request()
        self.endpoint.emit_stage("preflight")
        self.endpoint.emit_progress("inference", 2, 3)
        self.endpoint.emit_completed(candidate())
        events = []
        stream = BytesIO(self.output.getvalue())
        while True:
            event = worker_protocol.read_worker_message(stream)
            if event is None:
                break
            events.append(event)
        self.assertEqual([type(event) for event in events], [
            worker_protocol.StageEvent,
            worker_protocol.ProgressEvent,
            worker_protocol.CompletedEvent,
        ])
        for event in events:
            self.assertEqual(event.identity, self.request.identity)
            self.assertEqual(event.protocol, 1)
        with self.assertRaises(worker_protocol.ProtocolError):
            self.endpoint.emit_failed("internal", "too late")

    def test_cancelled_requires_a_matching_cancel(self):
        self.endpoint.read_request()
        with self.assertRaises(worker_protocol.ProtocolError):
            self.endpoint.emit_cancelled()


if __name__ == "__main__":
    unittest.main()
