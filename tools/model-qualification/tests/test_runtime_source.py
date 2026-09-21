import hashlib
import importlib.util
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest


MODULE_PATH = Path(__file__).resolve().parents[1] / "runtime_source.py"
SPEC = importlib.util.spec_from_file_location("deadpan_runtime_source", MODULE_PATH)
runtime_source = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = runtime_source
SPEC.loader.exec_module(runtime_source)


def manifest_for(root, files):
    entries = {}
    for relative, content in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        entries[relative] = {
            "bytes": len(content),
            "sha256": hashlib.sha256(content).hexdigest(),
        }
    return {"files": entries}


def module(path, search_path=None):
    values = {"__file__": str(path)}
    if search_path is not None:
        values["__path__"] = [str(value) for value in search_path]
    return SimpleNamespace(**values)


class RuntimeSourceTests(unittest.TestCase):
    def test_verify_tree_checks_every_pinned_source_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            files = {
                "packages/ltx-core-mlx/src/ltx_core_mlx/__init__.py": b"core init\n",
                "packages/ltx-core-mlx/src/ltx_core_mlx/lazy.py": b"lazy source\n",
                "packages/ltx-pipelines-mlx/src/ltx_pipelines_mlx/__init__.py": b"pipeline init\n",
            }
            manifest = manifest_for(root, files)
            checks = []

            runtime_source.verify_tree(root, manifest, lambda: checks.append(True))

            self.assertEqual(len(checks), len(files))
            self.assertEqual(
                {relative: entry["sha256"] for relative, entry in manifest["files"].items()},
                {
                    relative: hashlib.sha256(content).hexdigest()
                    for relative, content in files.items()
                },
            )

    def test_verify_tree_rejects_mutated_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            relative = "packages/ltx-core-mlx/src/ltx_core_mlx/__init__.py"
            manifest = manifest_for(root, {relative: b"pinned\n"})
            (root / relative).write_bytes(b"mutated\n")

            with self.assertRaises(ValueError):
                runtime_source.verify_tree(root, manifest, lambda: None)

    def test_verify_roots_requires_exact_checkout_package_origins(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            expected = {
                package: root / relative / "__init__.py"
                for package, relative in runtime_source.PACKAGE_PATHS.items()
            }
            calls = []

            def lookup(package):
                calls.append(package)
                return SimpleNamespace(origin=str(expected[package]))

            runtime_source.verify_roots(root, lookup)
            self.assertEqual(set(calls), set(expected))

            alternate = (
                root
                / ".venv"
                / "lib"
                / "python3.14"
                / "site-packages"
                / "ltx_core_mlx"
                / "__init__.py"
            )

            def lookup_alternate(package):
                if package == "ltx_core_mlx":
                    return SimpleNamespace(origin=str(alternate))
                return SimpleNamespace(origin=str(expected[package]))

            with self.assertRaises(ValueError):
                runtime_source.verify_roots(root, lookup_alternate)

    def test_loaded_sources_returns_all_verified_imported_source_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            files = {
                "packages/ltx-core-mlx/src/ltx_core_mlx/__init__.py": b"core init\n",
                "packages/ltx-core-mlx/src/ltx_core_mlx/lazy.py": b"lazy source\n",
                "packages/ltx-pipelines-mlx/src/ltx_pipelines_mlx/__init__.py": b"pipeline init\n",
            }
            manifest = manifest_for(root, files)
            core_root = root / runtime_source.PACKAGE_PATHS["ltx_core_mlx"]
            pipeline_root = root / runtime_source.PACKAGE_PATHS["ltx_pipelines_mlx"]
            modules = {
                "ltx_core_mlx": module(core_root / "__init__.py", [core_root]),
                "ltx_core_mlx.lazy": module(core_root / "lazy.py"),
                "ltx_pipelines_mlx": module(pipeline_root / "__init__.py", [pipeline_root]),
                "unrelated": module(Path(temporary) / "outside.py"),
            }

            sources = runtime_source.loaded_sources(root, manifest, modules)

            self.assertEqual(
                sources,
                {relative: manifest["files"][relative]["sha256"] for relative in files},
            )

    def test_loaded_sources_rejects_alternate_module_and_search_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            relative = "packages/ltx-core-mlx/src/ltx_core_mlx/__init__.py"
            manifest = manifest_for(root, {relative: b"core init\n"})
            alternate = (
                root
                / ".venv"
                / "lib"
                / "python3.14"
                / "site-packages"
                / "ltx_core_mlx"
                / "__init__.py"
            )
            alternate.parent.mkdir(parents=True)
            alternate.write_bytes(b"core init\n")

            with self.assertRaises(ValueError):
                runtime_source.loaded_sources(
                    root,
                    manifest,
                    {"ltx_core_mlx": module(alternate)},
                )

            core_root = root / runtime_source.PACKAGE_PATHS["ltx_core_mlx"]
            outside = Path(temporary) / "outside-package"
            with self.assertRaises(ValueError):
                runtime_source.loaded_sources(
                    root,
                    manifest,
                    {
                        "ltx_core_mlx": module(
                            core_root / "__init__.py", [core_root, outside]
                        )
                    },
                )

    def test_loaded_sources_rejects_unexpected_or_unattributable_ltx_modules(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            relative = "packages/ltx-core-mlx/src/ltx_core_mlx/__init__.py"
            manifest = manifest_for(root, {relative: b"core init\n"})
            core_init = root / relative

            with self.assertRaises(ValueError):
                runtime_source.loaded_sources(
                    root,
                    manifest,
                    {"ltx_unknown_family": module(core_init)},
                )

            with self.assertRaises(ValueError):
                runtime_source.loaded_sources(
                    root,
                    manifest,
                    {"ltx_core_mlx.missing_file": SimpleNamespace()},
                )

    def test_loaded_sources_rejects_lazy_module_hash_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            relative = "packages/ltx-core-mlx/src/ltx_core_mlx/lazy.py"
            manifest = manifest_for(root, {relative: b"pinned lazy source\n"})
            (root / relative).write_bytes(b"mutated lazy source\n")

            with self.assertRaises(ValueError):
                runtime_source.loaded_sources(
                    root,
                    manifest,
                    {"ltx_core_mlx.lazy": module(root / relative)},
                )

    def test_unrelated_modules_are_ignored(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            unrelated = SimpleNamespace(__file__=str(Path(temporary) / "unrelated.py"))

            self.assertEqual(
                runtime_source.loaded_sources(root, {"files": {}}, {"numpy": unrelated}),
                {},
            )


if __name__ == "__main__":
    unittest.main()
