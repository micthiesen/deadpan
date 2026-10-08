import builtins
import copy
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from test_extension_worker import request_wire, media, backend


VERSION = "0.15.8+deadpan-extension1"


class ProductionExtensionTests(unittest.TestCase):
    def request(self, direction="from_left", count=24, rate=24, generated=24):
        wire = request_wire(direction, count, rate, generated=generated, runtime_version=VERSION)
        wire["provider"]["pack_id"] = "ltx-2.3-q4-extension"
        return wire

    def validate(self, wire):
        return media.validate_extension_plan(wire["plan"], wire["constraints"]["video"], VERSION)

    def test_all_legal_counts_and_both_directions_are_bounded(self):
        for direction in ("from_left", "from_right"):
            for generated in range(8, 73, 8):
                wire = self.request(direction, generated, 24, generated)
                with self.subTest(direction=direction, generated=generated):
                    self.assertEqual(self.validate(wire), (generated, generated + 9))
                    self.assertEqual(media.extension_latent_counts(wire["plan"], wire["constraints"]["video"], VERSION),
                                     (2, generated // 8, 2 + generated // 8))
                    start = 9 if direction == "from_left" else 0
                    samples = list(media.extension_sample_positions(generated, 9, generated, direction))
                    self.assertEqual([lo for lo, _, _, _ in samples], list(range(start, start + generated)))
        for count, rate, generated in ((73, 24, 72), (181, 120, 40), (360, 120, 72), (1, 121, 8)):
            with self.subTest(count=count, rate=rate), self.assertRaises(ValueError):
                self.validate(self.request(count=count, rate=rate, generated=generated))

    def test_requested_duration_uses_nearest_legal_interval_and_shorter_ties(self):
        for count, expected in ((1, 8), (15, 8), (16, 16), (30, 24), (60, 48), (90, 72)):
            wire = self.request(count=count, rate=30, generated=expected)
            self.assertEqual(self.validate(wire), (count, 9 + expected))
            for wrong in (8, 16, 24, 48, 72):
                if wrong == expected:
                    continue
                changed = copy.deepcopy(wire)
                changed["plan"]["sampling"]["generated_frame_count"] = wrong
                with self.assertRaisesRegex(ValueError, "requires nine context"):
                    self.validate(changed)

    def test_development_and_production_pack_identities_do_not_cross(self):
        base = {"pack_id": "ltx-2.3-q4-extension", "pack_version": "1", "model_family": "ltx-2.3",
                "runtime_id": "ltx-mlx", "runtime_versions": [VERSION], "operations": ["extension_hold"], "files": []}
        with self.assertRaisesRegex(ValueError, "component inventory"):
            backend._model_manifest(base, Path("/unused"), lambda: None, False, "extension_hold")
        for pack_id, versions in (("ltx-2.3-q4-extension-development", [VERSION]),
                                  (base["pack_id"], ["0.15.8+deadpan-extension-dev2"]),
                                  (base["pack_id"], [VERSION, "0.15.8+deadpan-extension-dev2"]),
                                  ("ltx-2.3-q4-bridge", [VERSION])):
            changed = dict(base, pack_id=pack_id, runtime_versions=versions)
            with self.subTest(pack_id=pack_id, versions=versions), self.assertRaisesRegex(ValueError, "runtime compatibility"):
                backend._model_manifest(changed, Path("/unused"), lambda: None, False, "extension_hold")
        with self.assertRaisesRegex(ValueError, "unsupported bridge pack"):
            backend._model_manifest(base, Path("/unused"), lambda: None, False, "bridge_hold")


class OperationSmokeTests(unittest.TestCase):
    def run_smoke(self, operation, device="Device(gpu, 0)"):
        extension = operation == "extension_hold"
        pack = {"pack_id": "ltx-2.3-q4-extension" if extension else "ltx-2.3-q4-bridge",
                "pack_version": "1", "runtime_id": "ltx-mlx", "operations": [operation],
                "runtime_versions": [VERSION if extension else "0.15.8+deadpan5"]}
        paths = {"runtime_source": Path("/source"), "source_manifest": {}, "model_pack": pack,
                 "model_manifest_sha256": "a" * 64, "verified_assets": [object()]}
        mx = MagicMock()
        mx.arange.return_value.__mul__.return_value.sum.return_value.item.return_value = 357389824
        mx.default_device.return_value = device
        mx.__version__ = "0.31.1"
        imported = []
        real_import = builtins.__import__

        def import_module(name, globals=None, locals=None, fromlist=(), level=0):
            if name == "mlx.core":
                return SimpleNamespace(core=mx)
            if name.startswith("ltx_"):
                imported.append(name)
                return SimpleNamespace(**{item: object() for item in fromlist})
            return real_import(name, globals, locals, fromlist, level)

        with patch.object(backend, "runtime_paths", return_value=paths) as resolve, \
                patch.object(backend, "loaded_sources", return_value={"source": "hash"}), \
                patch("builtins.__import__", side_effect=import_module):
            report = backend.check_runtime({"model_pack": pack})
        self.assertEqual(resolve.call_args.kwargs, {"hash_assets": False, "operation": operation})
        selected = "retake" if extension else "keyframe_interpolation"
        other = "keyframe_interpolation" if extension else "retake"
        self.assertIn("ltx_pipelines_mlx." + selected, imported)
        self.assertNotIn("ltx_pipelines_mlx." + other, imported)
        self.assertEqual(report["schema_version"], 2)
        self.assertEqual(report["operation"], operation)
        self.assertEqual(report["runtime_version"], pack["runtime_versions"][0])
        self.assertEqual(report["model_manifest_sha256"], "a" * 64)

    def test_each_operation_imports_and_reports_its_own_pipeline(self):
        for operation in ("bridge_hold", "extension_hold"):
            with self.subTest(operation=operation):
                self.run_smoke(operation)

    def test_cpu_smoke_cannot_claim_metal(self):
        with self.assertRaisesRegex(ValueError, "Metal arithmetic"):
            self.run_smoke("extension_hold", "Device(cpu, 0)")

    def test_ambiguous_or_unsupported_operation_fails_before_runtime_resolution(self):
        for operations in ([], ["extension_hold", "bridge_hold"], ["transcribe"]):
            with patch.object(backend, "runtime_paths") as resolve, self.assertRaisesRegex(ValueError, "operation"):
                backend.check_runtime({"model_pack": {"operations": operations}})
            resolve.assert_not_called()
