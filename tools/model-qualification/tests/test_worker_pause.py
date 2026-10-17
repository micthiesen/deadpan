"""Scheduling controls with fake GPU synchronization; no MLX/model imports."""

from io import BytesIO
import copy
import importlib.util
import json
from pathlib import Path
import sys
import threading
import unittest

from test_worker_protocol_v2 import bridge_request_wire
import mlx_backend

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "deadpan_pause_protocol", Path(__file__).resolve().parents[1] / "worker_protocol.py")
protocol = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = protocol
SPEC.loader.exec_module(protocol)


class Cancelled(Exception):
    pass


class Output(BytesIO):
    def __init__(self):
        super().__init__()
        self.emitted = threading.Event()

    def flush(self):
        self.emitted.set()


def encoded(message):
    stream = BytesIO()
    protocol.write_host_message(stream, message)
    return stream.getvalue()


class PauseTests(unittest.TestCase):
    def setUp(self):
        self.request = bridge_request_wire()
        self.output = Output()
        self.endpoint = protocol.WorkerProtocol(BytesIO(encoded(self.request)), self.output)
        self.endpoint.read_request()

    def control(self, sequence=1, paused=True, **changes):
        message = {"operation": "set_paused", "protocol": self.request["protocol"],
                   "identity": self.request["identity"],
                   "cancellation_token": self.request["cancellation_token"],
                   "sequence": sequence, "paused": paused}
        message.update(changes)
        self.endpoint._reader = BytesIO(encoded(message))
        self.assertTrue(self.endpoint.read_control())

    def cancel(self):
        message = {key: self.request[key] for key in ("protocol", "identity", "cancellation_token")}
        message["operation"] = "cancel"
        self.endpoint._reader = BytesIO(encoded(message))
        self.endpoint.read_control()

    def check(self):
        if self.endpoint.cancel_requested:
            raise Cancelled()
        self.endpoint.check_control_error()

    def compute(self, synchronize):
        outcomes = []
        done = threading.Event()

        def run():
            try:
                self.endpoint.scheduling_checkpoint(self.check, synchronize)
                outcomes.append("resumed")
            except Exception as error:
                outcomes.append(error)
            finally:
                done.set()

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        self.addCleanup(lambda: self.endpoint.control_failed("test cleanup"))
        return outcomes, done, thread

    def events(self):
        stream = BytesIO(self.output.getvalue())
        result = []
        while (event := protocol.read_worker_message(stream)) is not None:
            result.append(event)
        return result

    def test_pause_ack_follows_sync_and_resume_exits_hold(self):
        self.control()
        synchronizing, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)

        def synchronize():
            synchronizing.set()
            self.assertTrue(release.wait(2))

        outcomes, done, thread = self.compute(synchronize)
        self.assertTrue(synchronizing.wait(2))
        self.assertEqual(self.events(), [])
        release.set()
        self.assertTrue(self.output.emitted.wait(2))
        self.assertEqual([(event.sequence, event.paused) for event in self.events()], [(1, True)])
        self.assertFalse(done.is_set())
        self.control(2, False)
        self.assertTrue(done.wait(2))
        thread.join()
        self.assertEqual(outcomes, ["resumed"])
        self.assertEqual([(event.sequence, event.paused) for event in self.events()], [(1, True), (2, False)])

    def test_superseding_resume_during_sync_never_reports_stale_hold(self):
        self.control()
        def synchronize():
            self.control(4, False)
        self.endpoint.scheduling_checkpoint(self.check, synchronize)
        self.assertEqual([(event.sequence, event.paused) for event in self.events()], [(4, False)])

    def test_controls_coalesce_before_boundary_without_gpu_work(self):
        for sequence in range(1, 100):
            self.control(sequence, bool(sequence % 2))
        self.control(100, False)
        self.endpoint.scheduling_checkpoint(self.check, lambda: self.fail("unneeded GPU sync"))
        self.assertEqual([(event.sequence, event.paused) for event in self.events()], [(100, False)])

    def test_cancel_and_control_failure_wake_a_held_worker(self):
        for cancel in [True, False]:
            with self.subTest(cancel=cancel):
                self.setUp()
                self.control()
                outcomes, done, thread = self.compute(lambda: None)
                self.assertTrue(self.output.emitted.wait(2))
                if cancel:
                    self.cancel()
                else:
                    self.endpoint.control_failed("malformed control")
                self.assertTrue(done.wait(2))
                thread.join()
                self.assertIsInstance(outcomes[0], Cancelled if cancel else protocol.ProtocolError)
                self.assertEqual(len(self.events()), 1)

    def test_cancel_during_sync_wins_before_ack(self):
        self.control()
        with self.assertRaises(Cancelled):
            self.endpoint.scheduling_checkpoint(self.check, self.cancel)
        self.assertEqual(self.events(), [])

    def test_stale_and_invalid_controls_are_rejected(self):
        self.control(9)
        for changes in [{"sequence": 9}, {"sequence": 8}, {"sequence": 0},
                        {"sequence": True}, {"sequence": 2**64}, {"paused": 1},
                        {"protocol": 1}, {"protocol": 3},
                        {"identity": {"request_id": "wrong", "attempt_id": "attempt-2"}},
                        {"cancellation_token": "wrong"}, {"extra": 1}]:
            with self.subTest(changes=changes), self.assertRaises(protocol.ProtocolError):
                self.control(**changes)

    def test_pause_wire_roundtrips_both_supported_protocols(self):
        for version in [2, 3]:
            request = copy.deepcopy(self.request)
            message = {"operation": "set_paused", "protocol": version,
                       "identity": request["identity"], "cancellation_token": request["cancellation_token"],
                       "sequence": 3, "paused": True}
            parsed = protocol.read_host_message(BytesIO(encoded(message)))
            self.assertEqual(parsed.to_wire(), message)
            event = {"event": "pause_state", "protocol": version, "identity": request["identity"],
                     "sequence": 3, "paused": False}
            self.assertEqual(protocol.parse_worker_message(event)._wire(), event)
        extension = json.loads((ROOT / "crates/deadpan-jobs/tests/fixtures/generate_extension_v3.json").read_text())
        self.request = extension
        self.endpoint = protocol.WorkerProtocol(BytesIO(encoded(extension)), self.output)
        self.endpoint.read_request()
        self.control(1, False)
        self.endpoint.scheduling_checkpoint(self.check, lambda: self.fail("unexpected sync"))
        self.assertEqual(self.events()[0].protocol, 3)

    def test_step_progress_follows_sync_and_never_claims_current_step_finished(self):
        calls = []
        synchronize = lambda: calls.append("synchronized")
        emit = lambda stage, completed, total: calls.append((stage, completed, total))
        mlx_backend._step_progress(0, 30, synchronize, emit)
        mlx_backend._step_progress(29, 30, synchronize, emit)
        mlx_backend._step_progress(19, 20, synchronize, emit, total=23)
        mlx_backend._step_progress(0, 3, synchronize, emit, offset=20, total=23)
        mlx_backend._step_progress(2, 3, synchronize, emit, offset=20, total=23)
        self.assertEqual(calls, [
            "synchronized", ("inference", 0, 30),
            "synchronized", ("inference", 29, 30),
            "synchronized", ("inference", 19, 23),
            "synchronized", ("inference", 20, 23),
            "synchronized", ("inference", 22, 23),
        ])
        with self.assertRaisesRegex(RuntimeError, "GPU failure"):
            mlx_backend._step_progress(1, 30,
                lambda: (_ for _ in ()).throw(RuntimeError("GPU failure")), emit)
        self.assertEqual(len(calls), 10)


if __name__ == "__main__":
    unittest.main()
