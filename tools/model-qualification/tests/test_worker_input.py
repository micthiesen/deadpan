import importlib.util
import os
from pathlib import Path
import tempfile
import unittest


MODULE_PATH = Path(__file__).resolve().parents[1] / "worker.py"
SPEC = importlib.util.spec_from_file_location("deadpan_model_qualification_worker", MODULE_PATH)
worker = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(worker)


class ContainedReadTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.root_path = Path(self.scratch.name) / "workspace"
        self.root_path.mkdir()
        self.root = os.open(
            self.root_path,
            os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
        )

    def tearDown(self):
        os.close(self.root)
        self.scratch.cleanup()

    def write(self, reference, data=b"deadpan-input"):
        path = self.root_path / reference
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def test_reads_regular_file_and_returns_frozen_bytes(self):
        path = self.write("inputs/context.json", b'{"schema_version":1}')
        frozen = worker.contained_read(self.root, "inputs/context.json", 1024)
        self.assertEqual(frozen, b'{"schema_version":1}')

        path.write_bytes(b'{"schema_version":2}')
        self.assertEqual(frozen, b'{"schema_version":1}')

    def test_retained_root_descriptor_ignores_path_replacement(self):
        original = self.write("input.bin", b"original")
        moved = Path(self.scratch.name) / "moved-workspace"
        self.root_path.rename(moved)
        self.root_path.mkdir()
        (self.root_path / "input.bin").write_bytes(b"replacement")

        self.assertEqual(worker.contained_read(self.root, "input.bin", 32), b"original")
        self.assertEqual(original.name, "input.bin")

    def test_rejects_lexical_escape_and_ambiguous_components(self):
        for reference in [
            "",
            "/etc/passwd",
            "../outside",
            "inputs/../outside",
            "./input",
            "inputs/./input",
            "inputs//input",
            "inputs/",
            "inputs\\input",
            "inputs/\0input",
        ]:
            with self.subTest(reference=reference):
                with self.assertRaises(ValueError):
                    worker.contained_read(self.root, reference, 1024)

    def test_rejects_symlink_parent_and_final_component(self):
        real = self.root_path / "real"
        real.mkdir()
        (real / "input.bin").write_bytes(b"input")
        (self.root_path / "linked-parent").symlink_to(real, target_is_directory=True)
        (self.root_path / "linked-file").symlink_to(real / "input.bin")

        for reference in ["linked-parent/input.bin", "linked-file"]:
            with self.subTest(reference=reference):
                with self.assertRaises((OSError, ValueError)):
                    worker.contained_read(self.root, reference, 1024)

    def test_rejects_hardlink_fifo_and_directory(self):
        original = self.write("original.bin", b"input")
        os.link(original, self.root_path / "hardlink.bin")
        os.mkfifo(self.root_path / "pipe")
        (self.root_path / "directory").mkdir()

        for reference in ["original.bin", "hardlink.bin", "pipe", "directory"]:
            with self.subTest(reference=reference):
                with self.assertRaises(ValueError):
                    worker.contained_read(self.root, reference, 1024)

    def test_rejects_empty_and_oversized_json_before_parsing(self):
        self.write("empty.json", b"")
        self.write("exact.json", b"null")
        self.write("oversized.json", b"[null]")

        with self.assertRaises(ValueError):
            worker.contained_read(self.root, "empty.json", 4)
        self.assertEqual(worker.contained_read(self.root, "exact.json", 4), b"null")
        with self.assertRaises(ValueError):
            worker.contained_read(self.root, "oversized.json", 4)


class StrictJsonTests(unittest.TestCase):
    def test_accepts_bounded_input_from_contained_read(self):
        self.assertEqual(
            worker.strict_json(b'{"schema_version":1,"nested":{"value":2}}'),
            {"schema_version": 1, "nested": {"value": 2}},
        )

    def test_rejects_duplicate_fields_at_any_depth(self):
        for payload in [
            b'{"value":1,"value":2}',
            b'{"outer":{"value":1,"value":2}}',
            b'[{"value":1,"value":2}]',
        ]:
            with self.subTest(payload=payload):
                with self.assertRaisesRegex(ValueError, "duplicate JSON field"):
                    worker.strict_json(payload)

    def test_rejects_nonfinite_json_numbers(self):
        for constant in [b"NaN", b"Infinity", b"-Infinity"]:
            for payload in [constant, b'{"value":' + constant + b"}"]:
                with self.subTest(payload=payload):
                    with self.assertRaisesRegex(ValueError, "nonfinite JSON value"):
                        worker.strict_json(payload)


if __name__ == "__main__":
    unittest.main()
