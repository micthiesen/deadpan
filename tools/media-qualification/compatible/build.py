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
import re
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
    opus = pins["opus"]
    dav1d = pins["dav1d"]
    checkasm = pins["checkasm"]
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
                 ("ffmpeg-devel.asc", pin["key_url"], pin["key_sha256"]),
                 (f"opus-{opus['version']}.tar.gz", opus["archive_url"], opus["archive_sha256"]),
                 (f"dav1d-{dav1d['version']}.tar.xz", dav1d["archive_url"], dav1d["archive_sha256"]),
                 (f"dav1d-{dav1d['version']}.tar.xz.asc", dav1d["archive_url"] + ".asc", dav1d["signature_sha256"]),
                 ("videolan-release.asc", dav1d["key_url"], dav1d["key_sha256"]),
                 (f"checkasm-{checkasm['commit']}.tar.gz", checkasm["archive_url"], checkasm["archive_sha256"])]
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
        # Dearmor and gpgv verify these pinned public keys without importing
        # them or opening an agent socket, whose macOS path limit is short.
        run(["gpg", "--batch", "--no-autostart", "--homedir", keyring, "--dearmor",
             "--output", keyring / "ffmpeg.gpg", downloads / "ffmpeg-devel.asc"])
        signature = run(["gpgv", "--homedir", keyring, "--keyring", keyring / "ffmpeg.gpg", "--status-fd", "1",
                         downloads / names[1][0], downloads / names[0][0]])
        if f"[GNUPG:] VALIDSIG {pin['signing_key_fingerprint']} " not in signature:
            raise RuntimeError("signature did not match the pinned FFmpeg release key")
        report["signature_verification"] = signature
        run(["gpg", "--batch", "--no-autostart", "--homedir", keyring, "--dearmor",
             "--output", keyring / "videolan.gpg", downloads / "videolan-release.asc"])
        dav1d_signature = run(["gpgv", "--homedir", keyring, "--keyring", keyring / "videolan.gpg", "--status-fd", "1",
                              downloads / names[5][0], downloads / names[4][0]])
        if f"[GNUPG:] VALIDSIG {dav1d['signing_key_fingerprint']} " not in dav1d_signature:
            raise RuntimeError("signature did not match the pinned VideoLAN release key")
        report["dav1d_signature_verification"] = dav1d_signature
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
        with tarfile.open(downloads / names[3][0]) as archive:
            archive.extractall(work, filter="data")
        opus_source = work / f"opus-{opus['version']}"
        opus_prefix = work / "opus-prefix"
        opus_env = dict(os.environ, MACOSX_DEPLOYMENT_TARGET="15.0",
                        CFLAGS="-O2 -arch arm64 -mmacosx-version-min=15.0",
                        LDFLAGS="-arch arm64 -mmacosx-version-min=15.0")
        run([opus_source / "configure", f"--prefix={opus_prefix}", "--disable-shared",
             "--enable-static", "--with-pic", "--disable-doc", "--enable-extra-programs",
             "--disable-deep-plc", "--disable-dred", "--disable-osce"], cwd=opus_source, env=opus_env, timeout=300)
        run(["make", f"-j{args.jobs}"], cwd=opus_source, env=opus_env, timeout=600)
        opus_tests = run(["make", "check", f"-j{args.jobs}"], cwd=opus_source, env=opus_env, timeout=600)
        total = re.search(r"# TOTAL:\s+(\d+)", opus_tests)
        if total is None or int(total[1]) == 0:
            raise RuntimeError("libopus build did not run its tests")
        run(["make", "install"], cwd=opus_source, env=opus_env)
        report["opus"] = {"archive_sha256": opus["archive_sha256"],
                          "static_library_sha256": digest(opus_prefix / "lib/libopus.a"),
                          "license_sha256": digest(opus_source / "COPYING"),
                          "config_header": (opus_source / "config.h").read_text()}
        with tarfile.open(downloads / names[4][0]) as archive:
            archive.extractall(work, filter="data")
        dav1d_source = work / f"dav1d-{dav1d['version']}"
        with tarfile.open(downloads / names[7][0]) as archive:
            archive.extractall(work, filter="data")
        checkasm_source = dav1d_source / "subprojects/checkasm"
        (work / f"checkasm-{checkasm['commit']}").rename(checkasm_source)
        dav1d_prefix = work / "dav1d-prefix"
        dav1d_build = work / "dav1d-build"
        dav1d_env = dict(opus_env, CC="clang", PKG_CONFIG_PATH="", PKG_CONFIG_LIBDIR="")
        report["meson_version"] = run(["meson", "--version"]).strip()
        run(["meson", "setup", dav1d_build, dav1d_source, f"--prefix={dav1d_prefix}",
             "--buildtype=release", "--default-library=static", "--wrap-mode=nodownload",
             "--force-fallback-for=checkasm",
             "-Db_staticpic=true", "-Denable_tools=false", "-Denable_examples=false",
             "-Denable_tests=true", "-Dtestdata_tests=false", "-Dxxhash_muxer=disabled"],
            env=dav1d_env, timeout=300)
        run(["meson", "compile", "-C", dav1d_build, "-j", str(args.jobs)], env=dav1d_env, timeout=600)
        run(["meson", "test", "-C", dav1d_build, "--print-errorlogs"], env=dav1d_env, timeout=600)
        dav1d_tests = [json.loads(line) for line in (dav1d_build / "meson-logs/testlog.json").read_text().splitlines()]
        if not dav1d_tests or any(test["result"] != "OK" for test in dav1d_tests):
            raise RuntimeError("dav1d upstream tests were absent, skipped or unsuccessful")
        if not any("checkasm" in test["name"] for test in dav1d_tests):
            raise RuntimeError("dav1d did not run its assembly-versus-scalar checks")
        run(["meson", "install", "-C", dav1d_build], env=dav1d_env)
        report["dav1d"] = {"archive_sha256": dav1d["archive_sha256"],
                           "static_library_sha256": digest(dav1d_prefix / "lib/libdav1d.a"),
                           "license_sha256": digest(dav1d_source / "COPYING"),
                           "tests": dav1d_tests,
                           "config_header": (dav1d_build / "config.h").read_text()}
        report["checkasm"] = {"archive_sha256": checkasm["archive_sha256"],
                              "license_sha256": digest(checkasm_source / "LICENSE"),
                              "scope": "upstream test dependency only; not shipped"}
        configure = [str(source / "configure"), f"--prefix={prefix}", "--enable-shared", "--disable-static",
                     "--enable-pic", "--disable-gpl", "--disable-nonfree", "--disable-version3",
                     "--disable-autodetect", "--disable-network", "--disable-doc", "--disable-debug",
                     "--enable-libopus", "--disable-decoder=opus", "--disable-encoder=libopus",
                     "--enable-libdav1d", "--disable-decoder=av1",
                     "--disable-ffmpeg", "--disable-ffplay", "--arch=aarch64", "--cpu=generic",
                     "--target-os=darwin", f"--sysroot={sdk}", "--enable-videotoolbox", "--enable-audiotoolbox",
                     "--extra-cflags=-arch arm64 -mmacosx-version-min=15.0",
                     "--extra-ldflags=-arch arm64 -mmacosx-version-min=15.0"]
        report["configure_argv"] = configure
        env = dict(os.environ, MACOSX_DEPLOYMENT_TARGET="15.0", PKG_CONFIG_PATH="",
                   PKG_CONFIG_LIBDIR=os.pathsep.join(str(p / "lib/pkgconfig") for p in [opus_prefix, dav1d_prefix]))
        run(configure, cwd=source, env=env, timeout=300)
        run(["make", f"-j{args.jobs}"], cwd=source, env=env, timeout=1800)
        run(["make", "install"], cwd=source, env=env, timeout=180)
        shutil.copytree(opus_prefix / "include/opus", prefix / "include/opus")
        shutil.copytree(dav1d_prefix / "include/dav1d", prefix / "include/dav1d")
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
