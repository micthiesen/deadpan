#!/usr/bin/env python3
"""Prepare the pinned ORT 1.31.0 arm64 build input without installing Python code."""

import argparse
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import urllib.parse
import urllib.request
import zipfile

USER_AGENT = "OpenAI File Downloader, XaiImageApiFetch/1.0"
COMMIT = "402979ac1bef89dcd55add41eaa7277c407599eb"
HEADER_ROOT = f"https://raw.githubusercontent.com/microsoft/onnxruntime/{COMMIT}/include/onnxruntime/core/session/"
HEADERS = {
    "onnxruntime_c_api.h": (422790, "43d584f596ef66820421317df32a79a95fc1d39ee46cb3971a870c08d8ba4baa"),
    "onnxruntime_cxx_api.h": (175424, "1e80d494c47091b07d84de60ad570326f82ed841c6de66e5b3eb7dce17bfc467"),
    "onnxruntime_cxx_inline.h": (164869, "908a50414c3f67f32fd556cd16f7779fa78380e12e9d08e44f5e6f66b965a7be"),
    "onnxruntime_float16.h": (17991, "88b242845d25981633a0bbd1c148e273cf8bfb016ea3f57c4af41a06530f72b0"),
    "onnxruntime_ep_c_api.h": (165249, "b64036772115dbc6f06a900487ce22d34e862100db03e2ad62210758e27a19b2"),
    "onnxruntime_error_code.h": (2775, "5ce3b054e798eced8d14f5b86e98692fd33470463f96194ce0700a2d53dd8721"),
}
LIBRARY = "libonnxruntime.1.31.0.dylib"
LIBRARY_PIN = (33037336, "9d2945b4de2ca594608f33b99498c8a81e6696e75da7ae9c67bc9ad3cc36c0d6")
WHEEL_NAME = "onnxruntime-1.31.0-cp313-cp313-macosx_14_0_arm64.whl"
WHEEL_URL = "https://files.pythonhosted.org/packages/e0/2b/117f94d73a3bac4276c285c47e384e1b3ea67b191aa4c7592df9d3f4a136/" + WHEEL_NAME
WHEEL_PIN = (20881803, "0ba02a44acb6203040354d9a1f160e3f37a43feac7bb05caa3e0ea545efed505")
WHEEL_MEMBER = "onnxruntime/capi/" + LIBRARY
ALIASES = ("libonnxruntime.dylib", "libonnxruntime.1.dylib")


def checked_copy(source, destination, pin):
    """Read at most the pinned length plus one; never publish unchecked data."""
    size, expected = pin
    digest = hashlib.sha256()
    count = 0
    while True:
        chunk = source.read(min(65536, size - count + 1))
        if not chunk:
            break
        count += len(chunk)
        if count > size:
            raise ValueError("input exceeds pinned byte length")
        digest.update(chunk)
        if destination is not None:
            destination.write(chunk)
    if count != size or digest.hexdigest() != expected:
        raise ValueError("input byte length or SHA-256 differs from compiled pin")


def regular_file(path, pin):
    # Opening a FIFO must not hang before its file type is known.
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as source:
        metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size != pin[0]:
            raise ValueError(f"not the pinned regular file: {path}")
        checked_copy(source, None, pin)


def real_directory(path):
    if not stat.S_ISDIR(path.lstat().st_mode):
        raise ValueError(f"not a real directory: {path}")


def verify(prefix):
    """Verify bytes, real directory structure and exact internal aliases."""
    real_directory(prefix)
    real_directory(prefix / "include")
    real_directory(prefix / "lib")
    for name, pin in HEADERS.items():
        regular_file(prefix / "include" / name, pin)
    regular_file(prefix / "lib" / LIBRARY, LIBRARY_PIN)
    for alias in ALIASES:
        path = prefix / "lib" / alias
        if not path.is_symlink() or os.readlink(path) != LIBRARY:
            raise ValueError(f"not the exact internal runtime alias: {path}")
    # Additional retained provenance is allowed, but no other symlink, FIFO,
    # device, or socket can hide inside an otherwise valid prefix.
    allowed = {prefix / "lib" / name for name in ALIASES}
    def unreadable(error):
        raise error
    for directory, dirs, files in os.walk(prefix, followlinks=False, onerror=unreadable):
        for name in dirs + files:
            path = Path(directory) / name
            kind = path.lstat().st_mode
            if path in allowed:
                continue
            if not (stat.S_ISREG(kind) or stat.S_ISDIR(kind)):
                raise ValueError(f"unexpected nonregular prefix entry: {path}")


class PinnedRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        parsed = urllib.parse.urlsplit(new_url)
        original = urllib.parse.urlsplit(request.full_url)
        if parsed.scheme != "https" or parsed.hostname != original.hostname:
            raise ValueError("download redirected outside its pinned HTTPS host")
        return super().redirect_request(request, fp, code, message, headers, new_url)


def download(url, path, pin):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    opener = urllib.request.build_opener(PinnedRedirect())
    with opener.open(request, timeout=60) as response, path.open("xb") as output:
        final = urllib.parse.urlsplit(response.geturl())
        if final.scheme != "https" or final.hostname != urllib.parse.urlsplit(url).hostname:
            raise ValueError("download escaped its pinned HTTPS host")
        declared = response.headers.get("Content-Length")
        if declared is not None and int(declared) != pin[0]:
            raise ValueError("download Content-Length differs from compiled pin")
        checked_copy(response, output, pin)
        output.flush()
        os.fsync(output.fileno())


def extract_library(wheel, output):
    regular_file(wheel, WHEEL_PIN)
    with zipfile.ZipFile(wheel) as archive:
        matches = [item for item in archive.infolist() if item.filename == WHEEL_MEMBER]
        if len(matches) != 1:
            raise ValueError("wheel must contain exactly one pinned runtime member")
        item = matches[0]
        mode = item.external_attr >> 16
        if (item.file_size != LIBRARY_PIN[0] or item.flag_bits & 1
                or stat.S_IFMT(mode) not in (0, stat.S_IFREG)):
            raise ValueError("wheel runtime member has invalid size, encryption or type")
        with archive.open(item) as source, output.open("xb") as destination:
            checked_copy(source, destination, LIBRARY_PIN)
            destination.flush()
            os.fsync(destination.fileno())
    output.chmod(0o644)


def provenance(prefix, existing):
    return {
        "schema": 1,
        "version": "1.31.0",
        "architecture": "macos-arm64",
        "prefix": str(prefix),
        "existing_verified": existing,
        "user_agent": USER_AGENT,
        "header_commit": COMMIT,
        "headers": {name: {"url": HEADER_ROOT + name, "bytes": pin[0], "sha256": pin[1]}
                    for name, pin in HEADERS.items()},
        "wheel": {"url": WHEEL_URL, "bytes": WHEEL_PIN[0], "sha256": WHEEL_PIN[1],
                  "extracted_member": WHEEL_MEMBER},
        "library": {"bytes": LIBRARY_PIN[0], "sha256": LIBRARY_PIN[1]},
        "scope": "Pinned build input only; wheel never installed or imported. No model data.",
    }


def publish_exclusive(staged, prefix):
    """macOS atomic directory publication that cannot replace an existing path."""
    if sys.platform != "darwin":
        raise ValueError("preparing this native prefix requires macOS exclusive rename")
    # Python exposes rename(), which may replace an empty directory. The fixed
    # macOS libc operation adds RENAME_EXCL; no downloaded code is loaded here.
    rename = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True).renamex_np
    rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(os.fsencode(staged), os.fsencode(prefix), 0x00000004) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(prefix))


def _open_lock(path):
    """Retain one lock inode; the kernel releases ownership after any exit."""
    fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, 0o600)
    try:
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f"not a regular preparation lock: {path}")
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return fd
    except BaseException:
        os.close(fd)
        raise


def prepare(prefix, verify_only=False):
    if not prefix.is_absolute() or ".." in prefix.parts:
        raise ValueError("--prefix must be an explicit absolute path without '..'")
    # Canonicalize existing parents (including macOS /tmp), never the selected
    # prefix itself: selecting a symlink must be a failure.
    if verify_only:
        prefix = prefix.parent.resolve(strict=True) / prefix.name
        verify(prefix)
        return provenance(prefix, True)
    prefix.parent.mkdir(parents=True, exist_ok=True)
    prefix = prefix.parent.resolve(strict=True) / prefix.name
    lock = prefix.parent / ("." + prefix.name + ".prepare-ort.lock")
    lock_fd = _open_lock(lock)
    try:
        if os.path.lexists(prefix):
            verify(prefix)
            return provenance(prefix, True)
        with tempfile.TemporaryDirectory(prefix=".prepare-ort-", dir=prefix.parent) as scratch:
            scratch = Path(scratch)
            staged = scratch / "prefix"
            (staged / "include").mkdir(parents=True)
            (staged / "lib").mkdir()
            wheel = scratch / WHEEL_NAME
            download(WHEEL_URL, wheel, WHEEL_PIN)
            extract_library(wheel, staged / "lib" / LIBRARY)
            for name, pin in HEADERS.items():
                download(HEADER_ROOT + name, staged / "include" / name, pin)
            for alias in ALIASES:
                (staged / "lib" / alias).symlink_to(LIBRARY)
            report = provenance(prefix, False)
            (staged / "provenance.json").write_text(json.dumps(report, indent=2) + "\n")
            verify(staged)
            # A cooperating preparer cannot race this publication; no existing
            # prefix is repaired, replaced or silently overwritten.
            if os.path.lexists(prefix):
                raise FileExistsError(f"prefix appeared during preparation: {prefix}")
            publish_exclusive(staged, prefix)
            directory_fd = os.open(prefix.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
            return report
    finally:
        os.close(lock_fd)
        # Never unlink a lock inode: another opener may already hold it.


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--verify-only", action="store_true", help="verify without downloading or writing")
    args = parser.parse_args(argv)
    try:
        report = prepare(args.prefix, args.verify_only)
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        parser.exit(1, f"prepare-ort: {error}\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
