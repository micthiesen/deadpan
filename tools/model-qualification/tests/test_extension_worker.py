import builtins
import copy
from fractions import Fraction
from io import BytesIO
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import worker_protocol as protocol
import worker_media as media
import worker
import mlx_backend as backend


def request_wire(direction="from_left", count=8, numerator=24, denominator=1):
    value = json.loads((ROOT / "crates/deadpan-jobs/tests/fixtures/generate_extension_v3.json").read_text())
    value["plan"]["native_dimensions"]["width"] = 768
    sampling = value["plan"]["sampling"]
    sampling.update(direction=direction, output_frame_count=count,
                    project_rate={"numerator": numerator, "denominator": denominator})
    value["constraints"].update(conditioning="extend_" + direction)
    value["constraints"]["video"].update(frames=count, width=768, frame_rate=copy.deepcopy(sampling["project_rate"]))
    value["provider"].update(pack_id="ltx-2.3-q4-extension-development", pack_version="1",
                             runtime_id="ltx-mlx", runtime_version="0.15.8+deadpan-extension-dev1")
    return value


def context_for(request, opposite=True):
    sampling = request["plan"]["sampling"]
    spacing = Fraction(**sampling["project_rate"]) / Fraction(**sampling["native_rate"])

    def entry(index, position):
        return {"picture": {"authored_black": {"clock": {
            "kind": "definition", "project_id": request["project_id"],
            "revision_id": request["revision_id"], "definition": "hold-definition",
            "position": {"numerator": str(position.numerator), "denominator": str(position.denominator)}}}},
            "frame": {"reference": f"inputs/context-{index}.png", "sha256": "a" * 64, "byte_length": 50},
            "content": None}
    entries = [entry(index, 20 + index * spacing) for index in range(9)]
    left = sampling["direction"] == "from_left"
    anchor = 20 + 8 * spacing if left else Fraction(20)
    other = entry("opposite", anchor + (sampling["output_frame_count"] + 1) * (1 if left else -1))
    return {"schema_version": 1, "operation": "extension", "model_color_space": worker.MODEL_COLOR_SPACE.copy(),
            "plan": copy.deepcopy(request["plan"]), "input_color_interpretation": "authored black, canonical sRGB",
            "context": entries, "presentation": {"x": 0, "y": 0, "width": 768, "height": 320},
            "opposite": {"status": "present_unconditioned", **other} if opposite else {"status": "absent"},
            "region": {"selection": "none"}}


class ExtensionProtocolTests(unittest.TestCase):
    def test_shared_rust_goldens_round_trip_without_loading_a_model(self):
        for file, parse in [("generate_extension_v3.json", protocol.parse_host_message),
                            ("completed_extension_v3.json", protocol.parse_worker_message)]:
            wire = json.loads((ROOT / "crates/deadpan-jobs/tests/fixtures" / file).read_text())
            self.assertEqual(parse(wire)._wire(), wire)

    def test_strict_discriminants_fields_counts_and_authored_binding(self):
        mutations = [
            lambda v: v.update(protocol=2), lambda v: v.update(operation="generate_bridge"),
            lambda v: v["plan"].update(extra=True), lambda v: v["plan"].update(schema_version=True),
            lambda v: v["plan"]["sampling"].update(policy="interior_only"),
            lambda v: v["plan"]["sampling"].update(direction="after"),
            lambda v: v["plan"]["sampling"].update(context_frame_count=0),
            lambda v: v["plan"]["sampling"].update(context_frame_count=8),
            lambda v: v["plan"]["sampling"].update(generated_frame_count=7),
            lambda v: v["plan"]["sampling"].update(generated_frame_count=2**32 - 8),
            lambda v: v["plan"]["sampling"].update(output_frame_count=True),
            lambda v: v["plan"]["sampling"].update(generated_start=9),
            lambda v: v["plan"]["sampling"]["project_rate"].update(denominator=0),
            lambda v: v["constraints"].update(conditioning="bridge"),
            lambda v: v["constraints"]["video"].update(frames=7),
            lambda v: v["constraints"]["video"].update(height=512),
        ]
        for mutate in mutations:
            value = request_wire()
            mutate(value)
            with self.subTest(mutation=mutate), self.assertRaises(protocol.ProtocolError):
                protocol.parse_host_message(value)
        wire = json.dumps(request_wire()).replace('{', '{"protocol":3,', 1).encode()
        with self.assertRaises(protocol.ProtocolError):
            protocol.read_frame(BytesIO(len(wire).to_bytes(4, "big") + wire))

    def test_v3_cancel_identity_and_terminal_event_scope(self):
        request = request_wire()
        cancel = {key: request[key] for key in ("protocol", "identity", "cancellation_token")}
        cancel["operation"] = "cancel"
        reader, writer = BytesIO(), BytesIO()
        protocol.write_frame(reader, request)
        protocol.write_frame(reader, cancel)
        reader.seek(0)
        endpoint = protocol.WorkerProtocol(reader, writer)
        endpoint.read_request()
        self.assertTrue(endpoint.read_cancel())
        endpoint.emit_cancelled()
        writer.seek(0)
        self.assertEqual(protocol.read_worker_message(writer).protocol, 3)
        with self.assertRaises(protocol.ProtocolError):
            endpoint.emit_stage("inference")
        for version in (1, 2):
            wire = json.loads((ROOT / "crates/deadpan-jobs/tests/fixtures/completed_extension_v3.json").read_text())
            wire["protocol"] = version
            with self.assertRaises(protocol.ProtocolError):
                protocol.parse_worker_message(wire)


class ExtensionMediaTests(unittest.TestCase):
    def test_measured_admission_is_separate_from_generic_wire(self):
        for direction in ("from_left", "from_right"):
            for count, num, den in [(8, 24, 1), (12, 60, 1), (9, 30000, 1001), (1, 3, 1)]:
                value = request_wire(direction, count, num, den)
                self.assertEqual(media.validate_extension_plan(value["plan"], value["constraints"]["video"]), (count, 17))
        for count, num, den in [(12, 24, 1), (10, 30000, 1001), (1, 1, 1), (41, 120, 1)]:
            value = request_wire(count=count, numerator=num, denominator=den)
            protocol.parse_host_message(value)
            with self.assertRaisesRegex(ValueError, "duration"):
                media.validate_extension_plan(value["plan"], value["constraints"]["video"])
        for field, value in [("context_frame_count", 1), ("generated_frame_count", 16)]:
            request = request_wire()
            request["plan"]["sampling"][field] = value
            protocol.parse_host_message(request)
            with self.assertRaisesRegex(ValueError, "measured"):
                media.validate_extension_plan(request["plan"], request["constraints"]["video"])

    def test_center_samples_exclude_context_and_round_half_up(self):
        expected = [Fraction(0), Fraction(1, 2), Fraction(7, 6), Fraction(11, 6),
                    Fraction(5, 2), Fraction(19, 6), Fraction(23, 6), Fraction(9, 2),
                    Fraction(31, 6), Fraction(35, 6), Fraction(13, 2), Fraction(7)]
        for direction, start in [("from_left", 9), ("from_right", 0)]:
            positions = list(media.extension_sample_positions(12, 9, 8, direction))
            self.assertEqual([Fraction(lo) + Fraction(num, den) for lo, hi, num, den in positions],
                             [start + position for position in expected])
            for count in (1, 3, 8, 12, 180):
                for lo, hi, num, den in media.extension_sample_positions(count, 9, 8, direction):
                    self.assertTrue(start <= lo <= hi < start + 8)
            self.assertEqual([lo for lo, hi, num, den in media.extension_sample_positions(8, 9, 8, direction)], list(range(start, start + 8)))
            lo, hi, num, den = next(media.extension_sample_positions(1, 9, 8, direction))
            self.assertEqual((lo, hi), (start + 3, start + 4))
            self.assertEqual(media.blend_channel(103, 104, num, den), 104)
        for args in [(0, 9, 8, "from_left"), (True, 9, 8, "from_left"), (12, 9, 8, "after"), (1, 97, 8, "from_right")]:
            with self.assertRaises(ValueError):
                list(media.extension_sample_positions(*args))

    def test_report_distinguishes_generated_time_from_complete_native_movie(self):
        timing = media.extension_timing(request_wire()["plan"]["sampling"])
        parsed = {name: Fraction(int(value["numerator"]), int(value["denominator"])) for name, value in timing.items()}
        self.assertEqual(parsed["requested_duration"], Fraction(1, 3))
        self.assertEqual(parsed["generated_duration"], Fraction(1, 3))
        self.assertEqual(parsed["native_movie_duration"], Fraction(17, 24))
        self.assertEqual(parsed["context_duration"], Fraction(9, 24))
        self.assertEqual(parsed["context_anchor_span"], Fraction(8, 24))
        self.assertEqual(parsed["speed_conversion"], 1)


class ExtensionContextTests(unittest.TestCase):
    def validate(self, wire, context):
        # The worker's imported class remains its own module identity even when
        # older protocol tests load a second copy under a custom module name.
        request = worker.GenerateExtensionRequest(**{
            key: getattr(protocol.parse_host_message(wire), key)
            for key in protocol.GenerateExtensionRequest.__dataclass_fields__})
        return worker.validate_extension_context(context, request, wire["constraints"]["video"])

    def test_chronology_exact_fractional_spacing_and_opposite_exclusion(self):
        for direction in ("from_left", "from_right"):
            for opposite in (True, False):
                wire = request_wire(direction, 9, 30000, 1001)
                context = context_for(wire, opposite)
                refs = self.validate(wire, context)
                self.assertEqual(refs, [entry["frame"] for entry in context["context"]])
                self.assertEqual(len(refs), 9)
                self.assertNotIn("inputs/context-opposite.png", [ref["reference"] for ref in refs])

    def test_changed_definition_order_anchor_geometry_and_budget_fail(self):
        mutations = [
            lambda c: c["context"].reverse(), lambda c: c["context"].pop(),
            lambda c: c["context"][3]["picture"]["authored_black"]["clock"].update(revision_id="other"),
            lambda c: c["context"][3]["picture"]["authored_black"]["clock"].update(definition="other"),
            lambda c: c["context"][3]["picture"]["authored_black"]["clock"]["position"].update(numerator="0"),
            lambda c: c["context"][0]["frame"].update(byte_length=16 * 1024 * 1024),
            lambda c: c["context"][0]["frame"].update(reference="../bad.png"),
            lambda c: c["context"][0].update(content={"x": 0, "y": 0, "width": 768, "height": 320}),
            lambda c: c["opposite"].update(status="conditioned"),
            lambda c: c["opposite"]["picture"]["authored_black"]["clock"]["position"].update(numerator="0"),
            lambda c: c["presentation"].update(x=1),
            lambda c: c["region"].update(anchor={}),
        ]
        for mutate in mutations:
            wire = request_wire()
            context = context_for(wire)
            mutate(context)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                self.validate(wire, context)

    def test_selected_region_has_one_anchor_and_captured_request_target(self):
        wire = request_wire()
        wire["constraints"]["region_target"] = "hand"
        context = context_for(wire)
        context["region"] = {"selection": "selected", "target": "hand", "label": "Hand", "target_sha256": "a" * 64,
                             "anchor": {"status": "unavailable", "reason": "not_original"}}
        self.validate(wire, context)
        context["region"]["target"] = "other"
        with self.assertRaises(ValueError):
            self.validate(wire, context)


class ExtensionBackendPreflightTests(unittest.TestCase):
    def test_unsupported_request_rejects_before_mlx_import(self):
        actual_import = builtins.__import__
        def guarded_import(name, *args, **kwargs):
            self.assertFalse(name == "mlx" or name.startswith("mlx."), "unsupported request loaded MLX")
            return actual_import(name, *args, **kwargs)
        for mutate in [lambda v: v["plan"]["sampling"].update(direction="sideways"),
                       lambda v: v["plan"]["sampling"].update(generated_frame_count=16),
                       lambda v: v["constraints"].update(conditioning="bridge")]:
            wire = request_wire()
            mutate(wire)
            with patch.object(builtins, "__import__", side_effect=guarded_import), self.assertRaises(ValueError):
                backend.generate_extension({"operation": "extension_hold"}, wire, {"plan": wire["plan"]}, [], None, lambda _: None, lambda: None, {})

    def test_bridge_pack_does_not_implicitly_advertise_extension(self):
        manifest = {"pack_id": "ltx-2.3-q4-bridge", "pack_version": "1", "model_family": "ltx-2.3",
                    "runtime_id": "ltx-mlx", "runtime_versions": ["0.15.8+deadpan5"], "operations": ["bridge_hold"], "files": []}
        with self.assertRaisesRegex(ValueError, "development extension"):
            backend._model_manifest(manifest, Path("/unused"), lambda: None, True, "extension_hold")
        manifest.update(pack_id="ltx-2.3-q4-extension-development", runtime_versions=["0.15.8+deadpan-extension-dev1"], operations=["extension_hold"])
        with self.assertRaisesRegex(ValueError, "unsupported bridge pack"):
            backend._model_manifest(manifest, Path("/unused"), lambda: None, True)
        with self.assertRaisesRegex(ValueError, "component inventory"):
            backend._model_manifest(manifest, Path("/unused"), lambda: None, True, "extension_hold")


if __name__ == "__main__":
    unittest.main()
