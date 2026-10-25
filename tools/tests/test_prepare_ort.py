"""Offline regressions for the pinned, data-only ORT build-input preparer."""

import hashlib
import importlib.util
import io
import os
from pathlib import Path
import select
import signal
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("prepare_ort", ROOT / "tools/prepare-ort.py")
ort = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ort)


def pin(data):
    return len(data), hashlib.sha256(data).hexdigest()


class PrepareOrtTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name).resolve()
        self.prefix = self.root / "qualified/prefix"
        self.library = b"pinned library data"
        self.header = b"pinned header data"
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr(ort.WHEEL_MEMBER, self.library)
            archive.writestr("../must-not-extract", b"unselected")
        self.wheel = stream.getvalue()
        self.patches = [
            mock.patch.object(ort, "LIBRARY_PIN", pin(self.library)),
            mock.patch.object(ort, "WHEEL_PIN", pin(self.wheel)),
            mock.patch.object(ort, "HEADERS", {"onnxruntime_c_api.h": pin(self.header)}),
        ]
        for patch in self.patches:
            patch.start()
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(lambda: [patch.stop() for patch in reversed(self.patches)])

    def download(self, url, path, expected):
        data = self.wheel if url == ort.WHEEL_URL else self.header
        with io.BytesIO(data) as source, path.open("xb") as output:
            ort.checked_copy(source, output, expected)

    def install(self):
        with mock.patch.object(ort, "download", side_effect=self.download):
            return ort.prepare(self.prefix)

    def test_atomic_data_only_install_then_existing_verification_needs_no_network(self):
        report = self.install()
        self.assertFalse(report["existing_verified"])
        self.assertEqual((self.prefix / "lib" / ort.LIBRARY).read_bytes(), self.library)
        self.assertFalse((self.root / "must-not-extract").exists())
        self.assertEqual(sorted(p.name for p in (self.prefix / "lib").iterdir()),
                         sorted([ort.LIBRARY, *ort.ALIASES]))
        before = (self.prefix / "provenance.json").read_bytes()
        # Retained provenance is not trusted as a verification authority.
        (self.prefix / "prefix.json").write_text('{"sha256":"made up"}')
        with mock.patch.object(ort, "download", side_effect=AssertionError("network")):
            self.assertTrue(ort.prepare(self.prefix)["existing_verified"])
            self.assertTrue(ort.prepare(self.prefix, verify_only=True)["existing_verified"])
        self.assertEqual((self.prefix / "provenance.json").read_bytes(), before)

    def test_corrupt_existing_prefix_is_never_repaired_or_overwritten(self):
        self.install()
        library = self.prefix / "lib" / ort.LIBRARY
        library.write_bytes(b"bad")
        with mock.patch.object(ort, "download", side_effect=AssertionError("network")):
            with self.assertRaises(ValueError):
                ort.prepare(self.prefix)
        self.assertEqual(library.read_bytes(), b"bad")

    def test_wrong_download_leaves_no_published_prefix_or_staging(self):
        def bad_download(url, path, expected):
            with io.BytesIO(b"wrong data") as source, path.open("xb") as output:
                ort.checked_copy(source, output, expected)
        with mock.patch.object(ort, "download", side_effect=bad_download):
            with self.assertRaises(ValueError):
                ort.prepare(self.prefix)
        self.assertFalse(self.prefix.exists())
        self.assertEqual([p.name for p in self.prefix.parent.iterdir()], [".prefix.prepare-ort.lock"])

    def test_symlink_prefix_header_alias_escape_and_fifo_fail_without_hanging(self):
        self.install()
        alias = self.root / "alias"
        alias.symlink_to(self.prefix, target_is_directory=True)
        with self.assertRaises(ValueError):
            ort.prepare(alias, verify_only=True)
        header = self.prefix / "include/onnxruntime_c_api.h"
        header.unlink()
        header.symlink_to(self.prefix / "lib" / ort.LIBRARY)
        with self.assertRaises(OSError):
            ort.verify(self.prefix)
        header.unlink()
        os.mkfifo(header)
        with self.assertRaises(ValueError):
            ort.verify(self.prefix)
        header.unlink()
        header.write_bytes(self.header)
        runtime_alias = self.prefix / "lib" / ort.ALIASES[0]
        runtime_alias.unlink()
        runtime_alias.symlink_to("../outside")
        with self.assertRaises(ValueError):
            ort.verify(self.prefix)
        runtime_alias.unlink()
        runtime_alias.symlink_to(ort.LIBRARY)
        os.mkfifo(self.prefix / "extra-fifo")
        with self.assertRaises(ValueError):
            ort.verify(self.prefix)

    def test_relative_prefix_and_symlink_lock_are_refused(self):
        with self.assertRaises(ValueError):
            ort.prepare(Path("relative"))
        self.prefix.parent.mkdir()
        lock = self.prefix.parent / ".prefix.prepare-ort.lock"
        lock.symlink_to(self.root / "unrelated")
        with self.assertRaises(OSError):
            ort.prepare(self.prefix)
        self.assertTrue(lock.is_symlink())

    def test_preexisting_unlocked_lock_is_reused_without_truncation_or_network(self):
        self.install()
        lock = self.prefix.parent / ".prefix.prepare-ort.lock"
        lock.write_text("stale marker from an interrupted older preparer")
        inode = lock.stat().st_ino
        with mock.patch.object(ort, "download", side_effect=AssertionError("network")):
            self.assertTrue(ort.prepare(self.prefix)["existing_verified"])
        self.assertEqual(lock.stat().st_ino, inode)
        self.assertEqual(lock.read_text(), "stale marker from an interrupted older preparer")

    def test_live_advisory_lock_is_refused_and_close_releases_the_same_inode(self):
        self.install()
        lock = self.prefix.parent / ".prefix.prepare-ort.lock"
        fd = ort._open_lock(lock)
        inode = lock.stat().st_ino
        try:
            with self.assertRaises(BlockingIOError):
                ort.prepare(self.prefix)
        finally:
            os.close(fd)
        self.assertTrue(ort.prepare(self.prefix)["existing_verified"])
        self.assertEqual(lock.stat().st_ino, inode)

    def test_fifo_lock_is_refused_without_blocking(self):
        self.prefix.parent.mkdir()
        lock = self.prefix.parent / ".prefix.prepare-ort.lock"
        os.mkfifo(lock)
        with self.assertRaises(ValueError):
            ort.prepare(self.prefix)

    def test_sigkill_of_owned_lock_holder_allows_restart_with_same_lock_inode(self):
        self.install()
        lock = self.prefix.parent / ".prefix.prepare-ort.lock"
        child_code = (
            "import importlib.util, pathlib, sys; "
            "s=importlib.util.spec_from_file_location('prepare_ort',sys.argv[1]); "
            "m=importlib.util.module_from_spec(s); s.loader.exec_module(m); "
            "fd=m._open_lock(pathlib.Path(sys.argv[2])); "
            "print('locked',flush=True); sys.stdin.read()"
        )
        child = subprocess.Popen(
            [sys.executable, "-B", "-c", child_code, str(ROOT / "tools/prepare-ort.py"), str(lock)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        try:
            self.assertTrue(select.select([child.stdout], [], [], 5)[0], "child did not acquire lock")
            self.assertEqual(child.stdout.readline(), b"locked\n")
            inode = lock.stat().st_ino
            with self.assertRaises(BlockingIOError):
                ort.prepare(self.prefix)
            child.kill()
            child.communicate(timeout=5)
            self.assertEqual(child.returncode, -signal.SIGKILL)
            with mock.patch.object(ort, "download", side_effect=AssertionError("network")):
                self.assertTrue(ort.prepare(self.prefix)["existing_verified"])
            self.assertEqual(lock.stat().st_ino, inode)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=5)

    def test_atomic_publication_never_replaces_even_an_empty_existing_directory(self):
        staged = self.root / "staged"
        staged.mkdir()
        (staged / "retained").write_text("staged")
        destination = self.root / "existing"
        destination.mkdir()
        with self.assertRaises(FileExistsError):
            ort.publish_exclusive(staged, destination)
        self.assertTrue((staged / "retained").is_file())
        self.assertEqual(list(destination.iterdir()), [])

    def test_archive_requires_unique_exact_regular_member(self):
        for kind in ("missing", "duplicate", "symlink", "fifo"):
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, "w") as archive:
                name = "wrong-member" if kind == "missing" else ort.WHEEL_MEMBER
                item = zipfile.ZipInfo(name)
                item.create_system = 3
                item.external_attr = ((stat.S_IFLNK if kind == "symlink" else stat.S_IFIFO if kind == "fifo" else stat.S_IFREG) | 0o644) << 16
                archive.writestr(item, self.library)
                if kind == "duplicate":
                    with self.assertWarns(UserWarning):
                        archive.writestr(name, self.library)
            wheel = self.root / (kind + ".whl")
            wheel.write_bytes(stream.getvalue())
            with mock.patch.object(ort, "WHEEL_PIN", pin(stream.getvalue())):
                with self.assertRaises(ValueError, msg=kind):
                    ort.extract_library(wheel, self.root / (kind + ".dylib"))

    def test_download_sets_user_agent_and_bounds_size_and_hash(self):
        class Response(io.BytesIO):
            headers = {"Content-Length": "4"}
            def geturl(self):
                return "https://raw.githubusercontent.com/pinned"
        opener = mock.Mock()
        opener.open.return_value = Response(b"data")
        with mock.patch.object(ort.urllib.request, "build_opener", return_value=opener):
            ort.download("https://raw.githubusercontent.com/pinned", self.root / "download", pin(b"data"))
        request = opener.open.call_args.args[0]
        self.assertEqual(request.get_header("User-agent"), ort.USER_AGENT)
        self.assertEqual(opener.open.call_args.kwargs, {"timeout": 60})
        for data in (b"dat", b"datum", b"evil"):
            with self.assertRaises(ValueError):
                ort.checked_copy(io.BytesIO(data), io.BytesIO(), pin(b"data"))

    def test_redirects_cannot_downgrade_https_or_change_host(self):
        handler = ort.PinnedRedirect()
        request = ort.urllib.request.Request("https://raw.githubusercontent.com/pinned")
        for target in ("http://raw.githubusercontent.com/pinned", "https://elsewhere.example/pinned"):
            with self.assertRaises(ValueError):
                handler.redirect_request(request, None, 302, "redirect", {}, target)


class RepositoryContractTests(unittest.TestCase):
    def test_runtime_and_all_six_header_pins_match_the_native_build(self):
        build = (ROOT / "native/deadpan-align/build.rs").read_text()
        self.assertEqual(len(ort.HEADERS), 6)
        for name, (_, digest) in ort.HEADERS.items():
            self.assertIn('"' + name + '"', build)
            self.assertIn('"' + digest + '"', build)
        self.assertIn('"' + ort.LIBRARY_PIN[1] + '"', build)

    def test_ci_prepares_and_exports_ort_before_lint_and_tests(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        prepare = workflow.index("python3 tools/prepare-ort.py")
        self.assertLess(prepare, workflow.index("- name: Lint"))
        self.assertLess(prepare, workflow.index("- name: Test"))
        self.assertIn('DEADPAN_ORT_PREFIX=$RUNNER_TEMP/deadpan-ort/prefix', workflow)
        self.assertIn("unittest discover -s tools/tests -p 'test_prepare_ort.py'", workflow)


if __name__ == "__main__":
    unittest.main()
