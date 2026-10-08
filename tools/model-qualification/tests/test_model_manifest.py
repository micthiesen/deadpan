import importlib.util
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


QUALIFICATION = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(QUALIFICATION))
SPEC = importlib.util.spec_from_file_location("deadpan_mlx_backend_manifest", QUALIFICATION / "mlx_backend.py")
mlx_backend = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = mlx_backend
SPEC.loader.exec_module(mlx_backend)


APPROVED = json.loads((QUALIFICATION.parents[1] / "models/packs/ltx-2.3-q4-bridge-1.json").read_text())


def model_pack():
    return {
        "pack_id": APPROVED["pack_id"],
        "pack_version": APPROVED["pack_version"],
        "model_family": APPROVED["model_family"],
        "runtime_id": APPROVED["runtime_id"],
        "runtime_versions": APPROVED["runtime_versions"],
        "operations": APPROVED["operations"],
        "files": [
            {"name": file["name"], "sha256": file["sha256"], "bytes": file["bytes"]}
            for file in APPROVED["files"]
        ],
    }


TENSOR_HEADER = {"weight": {"dtype": "F32", "shape": [2], "data_offsets": [0, 8]}}


def tensor_fixture(header=TENSOR_HEADER, payload=b"\x00" * 8):
    raw = json.dumps(header, sort_keys=True, separators=(",", ":")).encode()
    raw = raw.ljust(256, b" ")
    return len(raw).to_bytes(8, "little") + raw + payload


def fixture_schema():
    canonical = json.dumps(TENSOR_HEADER, sort_keys=True, separators=(",", ":")).encode()
    return 256, hashlib.sha256(canonical).hexdigest(), 1


def fixture_pack_files(manifest, root):
    """Small headers in sparse files retain the admission byte-size contract."""
    for file in manifest["files"]:
        path = root / file["name"]
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("wb") as stream:
            if file["name"].endswith(".safetensors"):
                stream.write(tensor_fixture())
            stream.truncate(file["bytes"])


class ModelManifestTests(unittest.TestCase):
    def run_manifest(self, manifest):
        with tempfile.TemporaryDirectory() as directory:
            return mlx_backend._model_manifest(manifest, Path(directory), lambda: None, False)

    def test_compiled_manifest_passes_contract_before_missing_files(self):
        with self.assertRaisesRegex(ValueError, "unsafe or missing model component directory"):
            self.run_manifest(model_pack())

    def test_new_pack_and_component_revisions_are_selected_from_the_manifest(self):
        manifest = model_pack()
        manifest["pack_version"] = "2"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = {}
            for file in manifest["files"]:
                parts = file["name"].split("/")
                revision = "a" * 40 if parts[0] == "mlx_ltx_q4_pack" else "b" * 40
                parts[1] = revision
                file["name"] = "/".join(parts)
                expected[parts[0]] = root / parts[0] / revision
            fixture_pack_files(manifest, root)
            schemas = {name: fixture_schema() for name in mlx_backend.TENSOR_SCHEMAS}
            with patch.object(mlx_backend, "TENSOR_SCHEMAS", schemas):
                model, gemma, verified = mlx_backend._model_manifest(
                    manifest, root, lambda: None, False
                )
            self.assertEqual(model, expected["mlx_ltx_q4_pack"])
            self.assertEqual(gemma, expected["mlx_gemma_default_text_encoder"])
            self.assertEqual(len(verified), len(manifest["files"]))
            repositories = {
                "mlx_ltx_q4_pack": "dgrauet/ltx-2.3-mlx-q4",
                "mlx_gemma_default_text_encoder": "mlx-community/gemma-3-12b-it-4bit",
            }
            # These receipts enter the host's strict AssetClaim parser. Keep
            # its repository/path identity even when component revisions move.
            for claim, file in zip(verified, manifest["files"], strict=True):
                prefix, _, relative = file["name"].split("/")
                self.assertEqual(claim, {"repository": repositories[prefix],
                                         "path": relative, "size": file["bytes"],
                                         "sha256": file["sha256"]})

    def test_smoke_rejects_same_size_updated_tensor_schema_before_activation(self):
        manifest = model_pack()
        manifest["pack_version"] = "2"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture_pack_files(manifest, root)
            changed = next(file for file in manifest["files"] if file["name"].endswith(".safetensors"))
            changed["sha256"] = "a" * 64  # An admitted signed update may replace weight bytes.
            path = root / changed["name"]
            with path.open("r+b") as stream:
                stream.write(tensor_fixture({"weight": {"dtype": "F32", "shape": [1, 2],
                                                        "data_offsets": [0, 8]}}))
            self.assertEqual(path.stat().st_size, changed["bytes"])
            schemas = {name: fixture_schema() for name in mlx_backend.TENSOR_SCHEMAS}
            with patch.object(mlx_backend, "TENSOR_SCHEMAS", schemas):
                with self.assertRaisesRegex(ValueError, "unsupported safetensors tensor schema"):
                    mlx_backend._model_manifest(manifest, root, lambda: None, False)

    def test_runtime_and_operation_are_exact(self):
        pins = json.loads((QUALIFICATION.parent / "ai-runtime/pins.json").read_text())
        self.assertEqual(pins["runtime_version"], "0.15.8+deadpan6")
        self.assertEqual(model_pack()["runtime_versions"], [pins["provider_runtime_versions"]["bridge_hold"]])
        extension = json.loads((QUALIFICATION.parents[1] / "models/packs/ltx-2.3-q4-extension-1.json").read_text())
        self.assertEqual(extension["runtime_versions"], [pins["provider_runtime_versions"]["extension_hold"]])
        for versions in (["0.15.8+deadpan4"], ["0.15.9"],
                         ["0.15.8+deadpan4", "0.15.8+deadpan5"]):
            manifest = model_pack()
            manifest["runtime_versions"] = versions
            with self.subTest(versions=versions), self.assertRaisesRegex(
                    ValueError, "unsupported bridge pack or runtime compatibility"):
                self.run_manifest(manifest)

        manifest = model_pack()
        manifest["operations"] = ["transcribe"]
        with self.assertRaisesRegex(ValueError, "unsupported bridge pack or runtime compatibility"):
            self.run_manifest(manifest)

    def test_quantization_config_cannot_change_under_a_valid_manifest(self):
        manifest = model_pack()
        config = next(file for file in manifest["files"] if file["name"].endswith("/quantize_config.json"))
        manifest["files"].remove(config)
        config["sha256"] = "f" * 64
        manifest["files"].insert(0, config)
        with self.assertRaisesRegex(ValueError, "unsupported model configuration change"):
            self.run_manifest(manifest)

    def test_component_paths_and_duplicate_files_are_rejected(self):
        manifest = model_pack()
        manifest["files"][0]["name"] = "mlx_ltx_q4_pack/../LICENSE"
        with self.assertRaisesRegex(ValueError, "unsafe or unsupported bridge component path"):
            self.run_manifest(manifest)

        manifest = model_pack()
        manifest["files"][1] = dict(manifest["files"][0])
        with self.assertRaisesRegex(ValueError, "unexpected or duplicate bridge component file"):
            self.run_manifest(manifest)


class TensorSchemaTests(unittest.TestCase):
    def test_payload_changes_keep_the_qualified_schema(self):
        for payload in (b"\x00" * 8, b"\x01" * 8):
            data = tensor_fixture(payload=payload)
            self.assertEqual(mlx_backend._tensor_schema(
                io.BytesIO(data), len(data), fixture_schema(), "fixture"), 1)

    def test_same_size_names_shapes_dtypes_and_offsets_cannot_change(self):
        alternatives = [
            {"renamed": dict(TENSOR_HEADER["weight"])},
            {"weight": {"dtype": "F32", "shape": [1, 2], "data_offsets": [0, 8]}},
            {"weight": {"dtype": "I32", "shape": [2], "data_offsets": [0, 8]}},
            {"weight": {"dtype": "F32", "shape": [2], "data_offsets": [8, 16]}},
            {"weight": dict(TENSOR_HEADER["weight"]), "__metadata__": {"lora_alpha": "1"}},
        ]
        for header in alternatives:
            with self.subTest(header=header):
                data = tensor_fixture(header)
                self.assertEqual(len(data), len(tensor_fixture()))
                with self.assertRaisesRegex(ValueError, "unsupported safetensors tensor schema"):
                    mlx_backend._tensor_schema(io.BytesIO(data), len(data), fixture_schema(), "fixture")

    def test_header_extent_and_truncation_are_bounded(self):
        for data, size in ((b"\xff" * 8, 1024),
                           (tensor_fixture()[:20], len(tensor_fixture())),
                           (tensor_fixture(), 12)):
            with self.subTest(size=size, actual=len(data)):
                with self.assertRaisesRegex(ValueError, "safetensors header"):
                    mlx_backend._tensor_schema(io.BytesIO(data), size, fixture_schema(), "fixture")

    def test_duplicate_fields_cannot_be_hidden_by_json_decoding(self):
        raw = b'{"weight":{"dtype":"I32","dtype":"F32","shape":[2],"data_offsets":[0,8]}}'
        data = (256).to_bytes(8, "little") + raw.ljust(256, b" ") + b"\x00" * 8
        with self.assertRaisesRegex(ValueError, "duplicate safetensors header field"):
            mlx_backend._tensor_schema(io.BytesIO(data), len(data), fixture_schema(), "fixture")


if __name__ == "__main__":
    unittest.main()
