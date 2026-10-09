import builtins
import copy
from fractions import Fraction
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import worker_protocol as protocol
import worker_media as media
import worker
import worker_extension_context as extension_context
import mlx_backend as backend


def request_wire(direction="from_left", count=8, numerator=24, denominator=1, *,
                 generated=8, runtime_version="0.15.8+deadpan-extension-dev1"):
    value = json.loads((ROOT / "crates/deadpan-jobs/tests/fixtures/generate_extension_v3.json").read_text())
    value["plan"]["native_dimensions"]["width"] = 768
    sampling = value["plan"]["sampling"]
    sampling.update(direction=direction, output_frame_count=count, generated_frame_count=generated,
                    project_rate={"numerator": numerator, "denominator": denominator})
    value["constraints"].update(conditioning="extend_" + direction)
    value["constraints"]["video"].update(frames=count, width=768, frame_rate=copy.deepcopy(sampling["project_rate"]))
    value["provider"].update(pack_id="ltx-2.3-q4-extension-development", pack_version="1",
                             runtime_id="ltx-mlx", runtime_version=runtime_version)
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
    result = {"schema_version": 2, "operation": "extension", "model_color_space": worker.MODEL_COLOR_SPACE.copy(),
            "plan": copy.deepcopy(request["plan"]), "input_color_interpretation": "authored black, canonical sRGB",
            "context": entries, "presentation": {"x": 0, "y": 0, "width": 768, "height": 320},
            "opposite": {"status": "present_unconditioned", **other} if opposite else {"status": "absent"},
            "region": {"selection": "none"}}
    relative = lambda position: {"numerator": str(position.numerator), "denominator": str(position.denominator)}
    samples = [{"position": relative(20 + index * spacing - anchor), "picture": {"kind": "authored_black"}}
               for index in range(9)]
    result["continuity"] = {
        "capture_policy": "deadpan-extension-context-1", "shot_rule": "deadpan-context-shots-1",
        "signature_encoding": "deadpan-context-signatures-1",
        "binding": {"duration": sampling["output_frame_count"], "frame_rate": copy.deepcopy(sampling["project_rate"]),
                    "canvas": [768, 320], "region": None,
                    "inputs": {"operation": "extension", "capture": {
                        "operation": "extension", "direction": sampling["direction"],
                        "native_rate": copy.deepcopy(sampling["native_rate"]), "context_frames": 9,
                        "policy": "temporal_context_v1"}, "samples": samples,
                        "opposite": {"position": relative(Fraction((sampling["output_frame_count"] + 1) * (1 if left else -1))),
                                     "picture": {"kind": "authored_black"}} if opposite else None,
                        "support": [{"start": copy.deepcopy(samples[0]["position"]),
                                     "end_exclusive": copy.deepcopy(samples[-1]["position"]),
                                     "first": {"kind": "authored_black"}, "last": {"kind": "authored_black"}}],
                        "terminal": copy.deepcopy(samples[-1])}},
        "pictures": [], "source_picture_counts": [None, None],
        "signatures": artifact("inputs/continuity.bin", signature_bytes(0))}
    return result


def artifact(reference, data):
    return {"reference": reference, "sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def signature_bytes(count):
    return b"DPSIG001" + struct.pack("<I", count) + (bytes([50]) * 1728 + struct.pack("<32I", 65536, *([0] * 31))) * count


def decoded_context(request, kind="original", source_count=9):
    context = context_for(request)
    identity = lambda ordinal: ({"kind": "original", "qualification": "b" * 64, "frame": ordinal}
                               if kind == "original" else {"kind": "generated", "sampled_object": {
                                   "content": {"algorithm": "blake3", "digest": "c" * 64}, "byte_length": 100},
                                   "frame": ordinal, "content_aspect": [12, 5]})
    entries = context["context"] + [context["opposite"]]
    for index, entry in enumerate(entries):
        clock = entry["picture"]["authored_black"]["clock"]
        value = {"clock": clock, "picture": {"source_frame": min(index, 8),
                 "pts": {"ticks": min(index, 8), "time_base": {"numerator": 1, "denominator": 24}},
                 "stream": {"codec": "h264", "pixel_format": "yuv420p", "width": 768, "height": 320,
                            "sample_aspect": [1, 1], "rotation_quarter_turns": 0, "decoded_sample_bits": 8,
                            "color": {"transfer": "bt709", "primaries": "bt709", "matrix": "bt709", "range": "limited"}},
                 "model_input": "rec709_to_srgb"}}
        if kind == "original":
            value.update(asset="original", qualification="b" * 64)
        else:
            value.update(sampled_asset="generated", sampled_object=copy.deepcopy(identity(0)["sampled_object"]),
                         provenance={"content": {"algorithm": "blake3", "digest": "d" * 64}, "byte_length": 80})
        entry["picture"] = {kind: value}
        entry["content"] = copy.deepcopy(context["presentation"])
    evidence = context["continuity"]
    inputs = evidence["binding"]["inputs"]
    for index, sample in enumerate(inputs["samples"]):
        sample["picture"] = identity(index)
    inputs["terminal"] = copy.deepcopy(inputs["samples"][-1])
    inputs["opposite"]["picture"] = identity(8)
    inputs["support"][0].update(first=identity(0), last=identity(7))
    evidence.update(pictures=[identity(index) for index in range(min(source_count, 134))],
                    source_picture_counts=[source_count, source_count])
    evidence["signatures"] = artifact("inputs/continuity.bin", signature_bytes(len(evidence["pictures"])))
    return context


def select_region(context, target="hand"):
    record = {"label": "Hand", "asset": "original",
              "span": {"start": {"ticks": 0, "time_base": {"numerator": 1, "denominator": 24}},
                       "end": {"ticks": 9, "time_base": {"numerator": 1, "denominator": 24}}},
              "region": {"center": [500000, 500000], "size": [200000, 200000]}}
    context["continuity"]["binding"]["region"] = {"id": target, "record": record}
    context["region"] = {"selection": "selected", "target": target, "label": "Hand",
                         "target_sha256": hashlib.sha256(json.dumps(record, separators=(",", ":")).encode()).hexdigest(),
                         "anchor": {"status": "unavailable", "reason": "not_original"}}


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
    def validate(self, value):
        return media.validate_extension_plan(value["plan"], value["constraints"]["video"],
                                             value["provider"]["runtime_version"])

    def test_measured_admission_is_separate_from_generic_wire(self):
        for direction in ("from_left", "from_right"):
            for count, num, den in [(8, 24, 1), (12, 60, 1), (9, 30000, 1001), (1, 3, 1)]:
                value = request_wire(direction, count, num, den)
                self.assertEqual(self.validate(value), (count, 17))
        for count, num, den in [(12, 24, 1), (10, 30000, 1001), (1, 1, 1), (41, 120, 1)]:
            value = request_wire(count=count, numerator=num, denominator=den)
            protocol.parse_host_message(value)
            with self.assertRaisesRegex(ValueError, "duration"):
                self.validate(value)
        for field, value in [("context_frame_count", 1), ("generated_frame_count", 16)]:
            request = request_wire()
            request["plan"]["sampling"][field] = value
            protocol.parse_host_message(request)
            with self.assertRaisesRegex(ValueError, "requires nine context"):
                self.validate(request)

    def test_one_second_experiment_is_bound_to_its_own_runtime(self):
        version = "0.15.8+deadpan-extension-dev2"
        for direction in ("from_left", "from_right"):
            for count, num, den in [(30, 30, 1), (24, 24, 1), (29, 30000, 1001), (120, 120, 1), (1, 1, 1)]:
                value = request_wire(direction, count, num, den, generated=24, runtime_version=version)
                protocol.parse_host_message(value)
                with self.subTest(direction=direction, count=count, num=num):
                    self.assertEqual(self.validate(value), (count, 33))
            for count, num, den in [(31, 30, 1), (30, 30000, 1001), (121, 120, 1), (1, 1, 2)]:
                value = request_wire(direction, count, num, den, generated=24, runtime_version=version)
                protocol.parse_host_message(value)
                with self.subTest(direction=direction, count=count, num=num), self.assertRaisesRegex(ValueError, "duration"):
                    self.validate(value)
        for version, generated in [("0.15.8+deadpan-extension-dev1", 24),
                                   ("0.15.8+deadpan-extension-dev2", 8),
                                   ("0.15.8+deadpan-extension-dev2", 16),
                                   ("0.15.8+deadpan-extension-dev2", 32)]:
            value = request_wire(generated=generated, runtime_version=version)
            protocol.parse_host_message(value)
            with self.subTest(version=version, generated=generated), self.assertRaisesRegex(ValueError, "requires nine context"):
                self.validate(value)
        for version in ("0.15.8+deadpan5", "0.15.8+deadpan-extension-dev3", "", None, []):
            value = request_wire()
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, "runtime identity"):
                media.validate_extension_plan(value["plan"], value["constraints"]["video"], version)

    def test_latent_counts_come_from_the_admitted_runtime_and_plan(self):
        for version, generated, expected in [("0.15.8+deadpan-extension-dev1", 8, (2, 1, 3)),
                                             ("0.15.8+deadpan-extension-dev2", 24, (2, 3, 5))]:
            for direction in ("from_left", "from_right"):
                value = request_wire(direction, generated=generated, runtime_version=version)
                with self.subTest(version=version, direction=direction):
                    self.assertEqual(media.extension_latent_counts(value["plan"], value["constraints"]["video"], version), expected)
                wrong = copy.deepcopy(value)
                wrong["plan"]["sampling"]["context_frame_count"] = 17
                with self.assertRaisesRegex(ValueError, "requires nine context"):
                    media.extension_latent_counts(wrong["plan"], wrong["constraints"]["video"], version)
                wrong = copy.deepcopy(value)
                wrong["plan"]["sampling"]["generated_frame_count"] = 24 if generated == 8 else 8
                with self.assertRaisesRegex(ValueError, "requires nine context"):
                    media.extension_latent_counts(wrong["plan"], wrong["constraints"]["video"], version)

    def test_one_second_samples_use_only_generated_pictures_in_both_directions(self):
        expected = [0, 7, 15, 23, 31, 39, 47, 55, 63, 71, 79, 87, 95, 103, 111,
                    119, 127, 135, 143, 151, 159, 167, 175, 183, 191, 199, 207, 215, 223, 230]
        for direction, start in [("from_left", 9), ("from_right", 0)]:
            generated = list(range(0, 240, 10))
            native = [255] * 9 + generated if start else generated + [255] * 9
            positions = list(media.extension_sample_positions(30, 9, 24, direction))
            self.assertEqual([media.blend_channel(native[lo], native[hi], num, den)
                              for lo, hi, num, den in positions], expected)
            for count in (1, 24, 29, 30, 120, 180):
                for lo, hi, num, den in media.extension_sample_positions(count, 9, 24, direction):
                    self.assertTrue(start <= lo <= hi < start + 24)
            self.assertEqual([lo for lo, hi, num, den in media.extension_sample_positions(24, 9, 24, direction)],
                             list(range(start, start + 24)))
            lo, hi, num, den = next(media.extension_sample_positions(1, 9, 24, direction))
            self.assertEqual((lo, hi, Fraction(num, den)), (start + 11, start + 12, Fraction(1, 2)))
            self.assertEqual(media.blend_channel(31, 32, num, den), 32)

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

    def test_one_second_clocks_exclude_the_nine_context_pictures(self):
        for direction in ("from_left", "from_right"):
            value = request_wire(direction, 30, 30, generated=24, runtime_version="0.15.8+deadpan-extension-dev2")
            timing = media.extension_timing(value["plan"]["sampling"])
            parsed = {name: Fraction(int(ratio["numerator"]), int(ratio["denominator"])) for name, ratio in timing.items()}
            self.assertEqual(parsed, {"requested_duration": Fraction(1), "generated_duration": Fraction(1),
                                      "native_movie_duration": Fraction(33, 24), "context_duration": Fraction(9, 24),
                                      "context_anchor_span": Fraction(8, 24), "speed_conversion": Fraction(1),
                                      "retime_deviation": Fraction(0)})


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

    def test_context_admission_checks_the_request_runtime_before_reading_inputs(self):
        for direction in ("from_left", "from_right"):
            for version, generated in [("0.15.8+deadpan-extension-dev1", 8),
                                       ("0.15.8+deadpan-extension-dev2", 24)]:
                wire = request_wire(direction, generated, 24, generated=generated, runtime_version=version)
                context = context_for(wire, opposite=False)
                with self.subTest(direction=direction, version=version):
                    self.assertEqual(self.validate(wire, context), [entry["frame"] for entry in context["context"]])
                wire["provider"]["runtime_version"] = ("0.15.8+deadpan-extension-dev2" if generated == 8
                                                        else "0.15.8+deadpan-extension-dev1")
                with self.assertRaisesRegex(ValueError, "requires nine context"):
                    self.validate(wire, context)

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
        select_region(context)
        self.validate(wire, context)
        context["region"]["target"] = "other"
        with self.assertRaises(ValueError):
            self.validate(wire, context)

    def test_decoded_original_and_generated_identities_bind_in_both_directions(self):
        for kind in ("original", "generated"):
            for direction in ("from_left", "from_right"):
                wire = request_wire(direction)
                context = decoded_context(wire, kind)
                self.assertEqual(len(self.validate(wire, context)), 9)
                extension_context.validate_signature_bytes(context["continuity"], signature_bytes(9))

    def test_extension_requires_the_canonical_decoded_color_conversion(self):
        wire = request_wire()
        for kind in ("original", "generated"):
            for which in ("context", "opposite"):
                mutations = [
                    lambda picture: picture.update(model_input="rec709_codes_as_srgb"),
                    lambda picture: picture.update(model_input="srgb_codes_unchanged"),
                    lambda picture: picture["stream"]["color"].update(transfer="srgb"),
                    lambda picture: picture["stream"]["color"].update(transfer="pq"),
                    lambda picture: picture["stream"]["color"].update(transfer="hlg"),
                    lambda picture: picture["stream"]["color"].update(transfer="linear"),
                    lambda picture: picture["stream"]["color"].update(primaries="bt2020"),
                    lambda picture: picture["stream"]["color"].update(primaries="display_p3"),
                    lambda picture: picture["stream"].update(rotation_quarter_turns=1),
                    lambda picture: picture["stream"].update(decoded_sample_bits=16),
                ]
                for index, mutate in enumerate(mutations):
                    context = decoded_context(wire, kind)
                    entry = context["context"][2] if which == "context" else context["opposite"]
                    mutate(entry["picture"][kind]["picture"])
                    with self.subTest(kind=kind, which=which, index=index), self.assertRaisesRegex(ValueError, "canonical SDR"):
                        self.validate(wire, context)
            context = decoded_context(wire, kind)
            for entry in context["context"] + [context["opposite"]]:
                picture = entry["picture"][kind]["picture"]
                picture["stream"]["color"].update(transfer="srgb")
                picture["model_input"] = "srgb_codes_unchanged"
            self.validate(wire, context)

    def test_fractional_clean_aperture_is_retained_and_strictly_bounded(self):
        ratio = lambda n, d=1: {"numerator": str(n), "denominator": str(d)}
        valid = [ratio(1, 4), ratio(1, 4), ratio(1535, 2), ratio(639, 2)]
        for direction in ("from_left", "from_right"):
            wire = request_wire(direction)
            context = decoded_context(wire)
            for entry in context["context"] + [context["opposite"]]:
                entry["picture"]["original"]["picture"]["stream"]["clean_aperture"] = copy.deepcopy(valid)
            self.validate(wire, context)
            for invalid in [[], valid[:3], [ratio(-1), *valid[1:]],
                            [ratio(0), ratio(0), ratio(769), ratio(1)],
                            [ratio((1 << 127)-1), ratio(0), ratio(1), ratio(1)],
                            [ratio(1, 0), *valid[1:]],
                            [ratio(0), ratio(0), ratio(0), ratio(1)]]:
                changed = copy.deepcopy(context)
                changed["context"][2]["picture"]["original"]["picture"]["stream"]["clean_aperture"] = invalid
                with self.subTest(direction=direction, invalid=invalid), self.assertRaises(ValueError):
                    self.validate(wire, changed)

    def test_schema_policy_binding_support_and_alias_mutations_fail(self):
        mutations = [
            lambda c: c.update(schema_version=1), lambda c: c.pop("continuity"),
            lambda c: c["continuity"].update(extra=True),
            lambda c: c["continuity"].pop("source_picture_counts"),
            lambda c: c["continuity"].update(capture_policy="old"),
            lambda c: c["continuity"].update(shot_rule="old"),
            lambda c: c["continuity"].update(signature_encoding="old"),
            lambda c: c["continuity"]["binding"].update(duration=True),
            lambda c: c["continuity"]["binding"].update(duration=9),
            lambda c: c["continuity"]["binding"].update(canvas=[640, 480]),
            lambda c: c["continuity"]["binding"]["frame_rate"].update(numerator=30),
            lambda c: c["continuity"]["binding"]["inputs"].update(operation="bridge"),
            lambda c: c["continuity"]["binding"]["inputs"].update(opposite=None),
            lambda c: c["continuity"]["binding"]["inputs"].update(support=[]),
            lambda c: c["continuity"]["binding"]["inputs"]["capture"].update(direction="from_right"),
            lambda c: c["continuity"]["binding"]["inputs"]["capture"].update(policy="other"),
            lambda c: c["continuity"]["binding"]["inputs"]["samples"][2]["position"].update(numerator="-5"),
            lambda c: c["continuity"]["binding"]["inputs"]["samples"][2]["position"].update(numerator="-6_0"),
            lambda c: c["continuity"]["binding"]["inputs"]["terminal"]["position"].update(numerator="1"),
            lambda c: c["continuity"]["binding"]["inputs"]["support"][0]["start"].update(numerator="-9"),
            lambda c: c["continuity"]["binding"]["inputs"]["support"][0]["end_exclusive"].update(numerator="1"),
            lambda c: c["continuity"]["binding"]["inputs"]["samples"][0]["picture"].update(extra=True),
            lambda c: c["continuity"].update(source_picture_counts=[None]),
            lambda c: c["continuity"].update(source_picture_counts=[0, None]),
            lambda c: c["continuity"].update(source_picture_counts=[None] * 8193),
            lambda c: c["continuity"].update(pictures=[{"kind": "authored_black"}]),
            lambda c: c["continuity"]["signatures"].update(byte_length=13),
            lambda c: c["continuity"]["signatures"].update(reference=c["context"][0]["frame"]["reference"]),
            lambda c: c["continuity"]["signatures"].update(reference=c["opposite"]["frame"]["reference"]),
            lambda c: c["context"][1]["frame"].update(reference=c["context"][0]["frame"]["reference"], sha256="b" * 64),
        ]
        for index, mutate in enumerate(mutations):
            wire, context = request_wire(), context_for(request_wire())
            mutate(context)
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.validate(wire, context)
        wire = request_wire()
        context = context_for(wire)
        context["continuity"]["signatures"]["reference"] = wire["input"]["manifest"]
        with self.assertRaisesRegex(ValueError, "alias"):
            self.validate(wire, context)

    def test_repeated_png_references_require_identical_artifact_declarations(self):
        wire = request_wire()
        for which in ("context", "opposite"):
            context = context_for(wire)
            entry = context["context"][1] if which == "context" else context["opposite"]
            entry["frame"] = copy.deepcopy(context["context"][0]["frame"])
            refs = self.validate(wire, context)
            self.assertEqual(len(refs), 9)
            if which == "context":
                self.assertEqual(refs[:2], [entry["frame"], entry["frame"]])
            for change in ({"sha256": "b" * 64}, {"byte_length": 51}):
                previous = copy.deepcopy(entry["frame"])
                entry["frame"].update(change)
                with self.subTest(which=which, change=change), self.assertRaisesRegex(ValueError, "contradictory"):
                    self.validate(wire, context)
                entry["frame"] = previous
            entry["frame"]["reference"] = wire["input"]["manifest"]
            with self.assertRaisesRegex(ValueError, "alias"):
                self.validate(wire, context)

    def test_source_identity_counts_and_complete_padded_coverage_are_required(self):
        mutations = [
            lambda c: c["continuity"]["pictures"].pop(),
            lambda c: c["continuity"]["pictures"].append(copy.deepcopy(c["continuity"]["pictures"][0])),
            lambda c: c["continuity"]["pictures"][0].update(frame=1000),
            lambda c: c["continuity"]["pictures"][0].update(frame=True),
            lambda c: c["continuity"]["pictures"][0].update(frame=2**64),
            lambda c: c["continuity"]["pictures"][0].update(qualification="f" * 64),
            lambda c: c["continuity"].update(source_picture_counts=[9, 10]),
            lambda c: c["continuity"].update(source_picture_counts=[8, 8]),
            lambda c: c["continuity"].update(source_picture_counts=[None, None]),
            lambda c: c["continuity"].update(source_picture_counts=[True, True]),
            lambda c: c["continuity"].update(source_picture_counts=[2**64, 2**64]),
            lambda c: c["continuity"]["binding"]["inputs"]["support"][0]["last"].update(qualification="f" * 64),
            lambda c: c["continuity"]["binding"]["inputs"]["support"][0]["last"].update(frame=1),
            lambda c: c["context"][3]["picture"]["original"].update(qualification="f" * 64),
            lambda c: c["context"][3]["picture"]["original"]["picture"].update(source_frame=2),
        ]
        for index, mutate in enumerate(mutations):
            wire = request_wire()
            context = decoded_context(wire)
            mutate(context)
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.validate(wire, context)
        # The physical source continues beyond the sparse PNG context. All 125
        # following pictures, clipped only at the source end, must be retained.
        context = decoded_context(wire, source_count=200)
        self.validate(wire, context)
        context["continuity"]["pictures"].pop()
        with self.assertRaisesRegex(ValueError, "missing or unrelated"):
            self.validate(wire, context)

    def test_generated_crop_identity_and_record_mutations_fail(self):
        wire = request_wire()
        for aspect in ([24, 10], [0, 1], [True, 1], [1], "12:5"):
            context = decoded_context(wire, "generated")
            context["continuity"]["pictures"][0]["content_aspect"] = aspect
            with self.subTest(aspect=aspect), self.assertRaises(ValueError):
                self.validate(wire, context)
        mutations = [lambda c: c["continuity"]["binding"].update(region=None),
                     lambda c: c["continuity"]["binding"]["region"].update(id="other"),
                     lambda c: c["continuity"]["binding"]["region"].update(record=None),
                     lambda c: c["continuity"]["binding"]["region"]["record"].update(extra=True),
                     lambda c: c["continuity"]["binding"]["region"]["record"].update(label="Changed"),
                     lambda c: c["continuity"]["binding"]["region"]["record"]["region"]["center"].__setitem__(0, 600000),
                     lambda c: c["continuity"]["binding"]["region"]["record"]["span"]["end"].update(ticks=0)]
        wire["constraints"]["region_target"] = "hand"
        for index, mutate in enumerate(mutations):
            context = context_for(wire)
            select_region(context)
            mutate(context)
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.validate(wire, context)

    def test_reader_opens_and_unique_signatures_share_one_budget(self):
        wire = request_wire()
        context = decoded_context(wire)
        evidence = context["continuity"]
        inputs = evidence["binding"]["inputs"]
        identity = copy.deepcopy(evidence["pictures"][0])
        for entry in context["context"] + [context["opposite"]]:
            entry["picture"]["original"]["picture"]["source_frame"] = 0
        for sample in inputs["samples"] + [inputs["opposite"], inputs["terminal"]]:
            sample["picture"] = copy.deepcopy(identity)
        evidence["pictures"] = [identity]
        evidence["signatures"] = artifact("inputs/continuity.bin", signature_bytes(1))
        for spans, succeeds in ((510, True), (511, False)):
            def coordinate(index):
                fraction = Fraction(-8) + Fraction(index * 8, spans)
                return {"numerator": str(fraction.numerator), "denominator": str(fraction.denominator)}
            inputs["support"] = [{"start": coordinate(index), "end_exclusive": coordinate(index + 1),
                                  "first": copy.deepcopy(identity), "last": copy.deepcopy(identity)}
                                 for index in range(spans)]
            evidence["source_picture_counts"] = [1] * (spans + 1)
            if succeeds:
                self.validate(wire, context)
            else:
                with self.assertRaisesRegex(ValueError, "picture-read budget"):
                    self.validate(wire, context)
        evidence["pictures"] *= 513
        with self.assertRaisesRegex(ValueError, "identities exceed"):
            self.validate(wire, context)

    def test_optional_target_fields_use_rust_canonical_hash(self):
        wire = request_wire()
        wire["constraints"]["region_target"] = "hand"
        context = context_for(wire)
        select_region(context)
        record = context["continuity"]["binding"]["region"]["record"]
        # Rust omits empty paths and null provenance, and reduces time bases.
        record.update(samples=[], corrections=[], provenance=None)
        for endpoint in record["span"].values():
            endpoint["time_base"] = {"numerator": 2, "denominator": 48}
        self.validate(wire, context)
        record["samples"] = [{"at": 2, "region": copy.deepcopy(record["region"]), "confidence": 900, "state": "tracked"}]
        with self.assertRaisesRegex(ValueError, "captured region identity"):
            self.validate(wire, context)


class ExtensionRetainedInputTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.path = Path(self.scratch.name)
        (self.path / "inputs").mkdir()
        self.root = os.open(self.path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        self.wire = request_wire()
        self.context = decoded_context(self.wire)
        self.pngs = []
        for index, entry in enumerate(self.context["context"] + [self.context["opposite"]]):
            data = b"distinct PNG input " + bytes([index])
            reference = entry["frame"]["reference"]
            (self.path / reference).write_bytes(data)
            entry["frame"] = artifact(reference, data)
            if index < 9:
                self.pngs.append(data)
        self.signatures = signature_bytes(9)
        self.signature_path = self.path / self.context["continuity"]["signatures"]["reference"]
        self.signature_path.write_bytes(self.signatures)

    def tearDown(self):
        os.close(self.root)
        self.scratch.cleanup()

    def read(self):
        request = worker.GenerateExtensionRequest(**{
            key: getattr(protocol.parse_host_message(self.wire), key)
            for key in protocol.GenerateExtensionRequest.__dataclass_fields__})
        return worker.read_extension_inputs(self.root, self.context, request, self.wire["constraints"]["video"], lambda: None)

    def test_only_context_pngs_are_returned_and_opposite_is_still_verified(self):
        self.assertEqual(self.read(), self.pngs)
        (self.path / self.context["opposite"]["frame"]["reference"]).write_bytes(b"changed opposite")
        with self.assertRaisesRegex(ValueError, "conditioning input hash or length"):
            self.read()

    def test_repeated_artifact_keeps_every_chronological_input(self):
        self.context["context"][1]["frame"] = copy.deepcopy(self.context["context"][0]["frame"])
        self.context["opposite"]["frame"] = copy.deepcopy(self.context["context"][0]["frame"])
        expected = self.pngs.copy()
        expected[1] = expected[0]
        self.assertEqual(self.read(), expected)
        self.assertEqual(len(self.read()), 9)

    def test_signature_missing_tampered_oversized_and_symlink_files_fail(self):
        self.signature_path.unlink()
        with self.assertRaises(FileNotFoundError):
            self.read()
        for data in (self.signatures[:-1], self.signatures + b"x", b"x" + self.signatures[1:],
                     b"x" * (extension_context.MAX_SIGNATURE_BYTES + 1)):
            self.signature_path.write_bytes(data)
            with self.subTest(length=len(data)), self.assertRaises(ValueError):
                self.read()
        self.signature_path.unlink()
        self.signature_path.symlink_to(self.path / self.context["context"][0]["frame"]["reference"])
        with self.assertRaises(OSError):
            self.read()

    def test_binary_shape_is_checked_even_when_the_hash_matches(self):
        rows = [b"BADMAGIC" + self.signatures[8:], self.signatures[:8] + struct.pack("<I", 8) + self.signatures[12:]]
        for bins in ([65537] + [0] * 31, [0] * 32, [65536, 17] + [0] * 30):
            rows.append(self.signatures[:1740] + struct.pack("<32I", *bins) + self.signatures[1868:])
        for data in rows:
            self.signature_path.write_bytes(data)
            self.context["continuity"]["signatures"] = artifact("inputs/continuity.bin", data)
            with self.subTest(header=data[:12]), self.assertRaisesRegex(ValueError, "signature (header|histogram)"):
                self.read()

    def test_empty_signature_header_for_authored_black_is_valid(self):
        evidence = context_for(request_wire())["continuity"]
        extension_context.validate_signature_bytes(evidence, signature_bytes(0))

    def test_signature_verification_precedes_png_reads_and_obeys_cancellation(self):
        self.signature_path.write_bytes(b"bad")
        with patch.object(worker, "contained_read", wraps=worker.contained_read) as read:
            with self.assertRaisesRegex(ValueError, "signature hash or length"):
                self.read()
            self.assertEqual(read.call_count, 1)
            self.assertEqual(read.call_args.args[1], "inputs/continuity.bin")
        request = worker.GenerateExtensionRequest(**{
            key: getattr(protocol.parse_host_message(self.wire), key)
            for key in protocol.GenerateExtensionRequest.__dataclass_fields__})
        def cancel():
            raise worker.Cancelled()
        with patch.object(worker, "contained_read") as read, self.assertRaises(worker.Cancelled):
            worker.read_extension_inputs(self.root, self.context, request, self.wire["constraints"]["video"], cancel)
        read.assert_not_called()

    def test_hardlink_alias_to_signature_is_rejected(self):
        os.link(self.signature_path, self.path / "alias.bin")
        with self.assertRaisesRegex(ValueError, "unsafe"):
            self.read()

    def test_histogram_rounding_limits_are_inclusive(self):
        for total in (65520, 65552):
            bins = [total // 2, total - total // 2] + [0] * 30
            data = self.signatures[:1740] + struct.pack("<32I", *bins) + self.signatures[1868:]
            evidence = copy.deepcopy(self.context["continuity"])
            evidence["signatures"] = artifact("inputs/continuity.bin", data)
            extension_context.validate_signature_bytes(evidence, data)


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
                backend.generate_extension({"operation": "extension_hold", "model_pack": {
                    "runtime_versions": [wire["provider"]["runtime_version"]]}}, wire,
                    {"plan": wire["plan"]}, [], None, lambda _: None, lambda: None, {})

    def test_selected_runtime_and_request_envelope_must_match_before_loading_mlx(self):
        actual_import = builtins.__import__
        def guarded_import(name, *args, **kwargs):
            self.assertFalse(name == "mlx" or name.startswith("mlx."), "mismatched runtime loaded MLX")
            return actual_import(name, *args, **kwargs)
        dev1, dev2 = "0.15.8+deadpan-extension-dev1", "0.15.8+deadpan-extension-dev2"
        for request_version, selected_versions, generated, error in [
                (dev1, [dev1], 24, "requires nine context"),
                (dev2, [dev2], 8, "requires nine context"),
                (dev1, [dev2], 8, "selected immutable model pack"),
                (dev2, [dev1], 24, "selected immutable model pack"),
                (dev2, [dev1, dev2], 24, "selected immutable model pack")]:
            for direction in ("from_left", "from_right"):
                wire = request_wire(direction, generated=generated, runtime_version=request_version)
                paths = {"operation": "extension_hold", "model_pack": {"runtime_versions": selected_versions}}
                with self.subTest(version=request_version, selected=selected_versions, direction=direction), \
                        patch.object(builtins, "__import__", side_effect=guarded_import), \
                        self.assertRaisesRegex(ValueError, error):
                    backend.generate_extension(paths, wire, {"plan": wire["plan"]}, [], None, lambda _: None, lambda: None, {})

    def test_bridge_pack_does_not_implicitly_advertise_extension(self):
        manifest = {"pack_id": "ltx-2.3-q4-bridge", "pack_version": "1", "model_family": "ltx-2.3",
                    "runtime_id": "ltx-mlx", "runtime_versions": ["0.15.8+deadpan5"], "operations": ["bridge_hold"], "files": []}
        with self.assertRaisesRegex(ValueError, "development extension"):
            backend._model_manifest(manifest, Path("/unused"), lambda: None, True, "extension_hold")
        manifest.update(pack_id="ltx-2.3-q4-extension-development", operations=["extension_hold"])
        for version in ("0.15.8+deadpan-extension-dev1", "0.15.8+deadpan-extension-dev2"):
            manifest["runtime_versions"] = [version]
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, "unsupported bridge pack"):
                    backend._model_manifest(manifest, Path("/unused"), lambda: None, True)
                with self.assertRaisesRegex(ValueError, "component inventory"):
                    backend._model_manifest(manifest, Path("/unused"), lambda: None, True, "extension_hold")
        for versions in (["0.15.8+deadpan-extension-dev1", "0.15.8+deadpan-extension-dev2"],
                         ["0.15.8+deadpan-extension-dev3"], ["0.15.8+deadpan5"], []):
            manifest["runtime_versions"] = versions
            with self.subTest(versions=versions), self.assertRaisesRegex(ValueError, "development extension"):
                backend._model_manifest(manifest, Path("/unused"), lambda: None, True, "extension_hold")


if __name__ == "__main__":
    unittest.main()
