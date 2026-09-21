#!/usr/bin/env python3
"""Build a signed, pinned, LGPL FFmpeg in a fresh isolated developer prefix."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent
USER_AGENT = "OpenAI File Downloader, XaiImageApiFetch/1.0"


def digest(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--download-cache", type=Path)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("this qualification targets Apple Silicon macOS")
    if not 1 <= args.jobs <= 16:
        parser.error("jobs must be between 1 and 16")
    work = args.work.resolve() if args.work else Path(tempfile.mkdtemp(prefix="deadpan-media-compatible-", dir="/tmp"))
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()):
        parser.error("work directory must be empty")
    pins = json.loads((ROOT / "pins.json").read_text())
    pin = pins["ffmpeg"]
    report = {"schema_version": 1, "started_utc": datetime.now(timezone.utc).isoformat(),
              "scope": "isolated developer build; not distribution or OS matrix qualification",
              "work_directory": str(work), "pins": pins, "commands": [], "result": "failed"}

    def run(argv, *, cwd=work, timeout=120, env=None):
        number = len(report["commands"])
        log = work / f"command-{number:02d}.log"
        started = time.monotonic()
        with log.open("w") as output:
            result = subprocess.run([str(v) for v in argv], cwd=cwd, env=env,
                                    stdout=output, stderr=subprocess.STDOUT, timeout=timeout, check=False)
        report["commands"].append({"argv": [str(v) for v in argv], "cwd": str(cwd),
                                   "exit_code": result.returncode, "log": str(log),
                                   "log_sha256": digest(log), "elapsed_seconds": round(time.monotonic() - started, 3)})
        if result.returncode:
            raise RuntimeError(f"command failed: {argv[0]} (see {log})")
        return log.read_text()

    try:
        downloads = work / "downloads"
        downloads.mkdir()
        names = [(f"ffmpeg-{pin['version']}.tar.xz", pin["archive_url"], pin["archive_sha256"]),
                 (f"ffmpeg-{pin['version']}.tar.xz.asc", pin["archive_url"] + ".asc", pin["signature_sha256"]),
                 ("ffmpeg-devel.asc", pin["key_url"], pin["key_sha256"])]
        for name, url, expected in names:
            target = downloads / name
            cached = args.download_cache / name if args.download_cache else None
            if cached and cached.is_file():
                shutil.copyfile(cached, target)
            else:
                request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
                with urllib.request.urlopen(request, timeout=120) as response, target.open("wb") as output:
                    shutil.copyfileobj(response, output)
            if digest(target) != expected:
                raise RuntimeError(f"SHA-256 mismatch: {name}")
        keyring = work / "gnupg"
        keyring.mkdir(mode=0o700)
        run(["gpg", "--batch", "--homedir", keyring, "--import", downloads / "ffmpeg-devel.asc"])
        signature = run(["gpg", "--batch", "--homedir", keyring, "--status-fd", "1", "--verify",
                         downloads / names[1][0], downloads / names[0][0]])
        if f"[GNUPG:] VALIDSIG {pin['signing_key_fingerprint']} " not in signature:
            raise RuntimeError("signature did not match the pinned FFmpeg release key")
        report["signature_verification"] = signature
        with tarfile.open(downloads / names[0][0]) as archive:
            archive.extractall(work, filter="data")
        source = work / f"ffmpeg-{pin['version']}"
        prefix = work / "prefix"
        sdk = run(["xcrun", "--show-sdk-path"]).strip()
        report["host"] = {"platform": platform.platform(), "macos": run(["sw_vers"]),
                          "processor": run(["sysctl", "-n", "machdep.cpu.brand_string"]).strip(),
                          "memory_bytes": int(run(["sysctl", "-n", "hw.memsize"]))}
        report["compiler"] = run(["clang", "--version"])
        report["sdk_version"] = run(["xcrun", "--show-sdk-version"]).strip()
        report["prefix"] = str(prefix)
        configure = [str(source / "configure"), f"--prefix={prefix}", "--enable-shared", "--disable-static",
                     "--enable-pic", "--disable-gpl", "--disable-nonfree", "--disable-version3",
                     "--disable-autodetect", "--disable-network", "--disable-doc", "--disable-debug",
                     "--disable-ffmpeg", "--disable-ffplay", "--arch=aarch64", "--cpu=generic",
                     "--target-os=darwin", f"--sysroot={sdk}", "--enable-videotoolbox", "--enable-audiotoolbox",
                     "--extra-cflags=-arch arm64 -mmacosx-version-min=15.0",
                     "--extra-ldflags=-arch arm64 -mmacosx-version-min=15.0"]
        report["configure_argv"] = configure
        env = dict(os.environ, MACOSX_DEPLOYMENT_TARGET="15.0", PKG_CONFIG_PATH="", PKG_CONFIG_LIBDIR="/nonexistent")
        run(configure, cwd=source, env=env, timeout=300)
        run(["make", f"-j{args.jobs}"], cwd=source, env=env, timeout=1800)
        run(["make", "install"], cwd=source, env=env, timeout=180)
        report["configuration"] = (source / "ffbuild/config.mak").read_text()
        report["license_file_sha256"] = {name: digest(source / name) for name in
                                         ("COPYING.LGPLv2.1", "LICENSE.md")}
        report["license_text"] = (source / "LICENSE.md").read_text()
        report["ffprobe_version"] = run([prefix / "bin/ffprobe", "-version"])
        report["libraries"] = {}
        for library in sorted((prefix / "lib").glob("*.dylib")):
            if library.is_symlink():
                continue
            report["libraries"][library.name] = {"path": str(library), "sha256": digest(library),
                                                 "linked_libraries": run(["otool", "-L", library]),
                                                 "build_version": run(["vtool", "-show-build", library])}
        report["result"] = "passed"
    finally:
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        report["source_sha256"] = {name: digest(ROOT / name) for name in ("build.py", "pins.json")}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"result": report["result"], "work": str(work), "report": str(args.output)}), flush=True)


if __name__ == "__main__":
    main()
