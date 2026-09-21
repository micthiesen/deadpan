#!/usr/bin/env python3
"""Instrument and test the native converter adapter on Apple Silicon macOS.

Native C dependencies such as BLAKE3 SIMD are also instrumented. Rust and the
separately built FFmpeg libraries are not. This is a developer qualification
tool; neither Python nor Clang is an end-user dependency.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import time


def run_test_command(command, *, cwd, env, log, timeout=600):
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
    except OSError as error:
        return 1, f"failed to start Cargo: {error}"
    try:
        return process.wait(timeout=timeout), None
    except subprocess.TimeoutExpired:
        failure = f"Cargo tests exceeded {timeout} seconds; stopped their process group"
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except OSError as error:
            failure += f"; group cleanup failed: {error}"
            process.kill()
        return process.wait(), failure


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("this qualification targets Apple Silicon macOS")
    prefix = os.environ.get("DEADPAN_FFMPEG_PREFIX")
    if not prefix or not Path(prefix).is_absolute():
        parser.error("export an absolute DEADPAN_FFMPEG_PREFIX first")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()):
        parser.error("work directory must be empty")
    repo = Path(__file__).resolve().parents[3]
    runtime = Path(subprocess.check_output(
        ["clang", "-print-file-name=libclang_rt.asan_osx_dynamic.dylib"], text=True
    ).strip())
    if not runtime.is_absolute() or not runtime.is_file():
        parser.error("Clang did not provide its Darwin ASan/UBSan runtime")
    flags = ["-C", f"link-arg={runtime}", "-C", f"link-arg=-Wl,-rpath,{runtime.parent}"]
    env = dict(os.environ)
    env.pop("RUSTFLAGS", None)
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    env["CFLAGS"] = "-fsanitize=address,undefined -fno-omit-frame-pointer -fno-sanitize-recover=all"
    # An explicit target confines link flags to target crates. Applying the ASan
    # runtime to host proc-macro dylibs loads interceptors too late inside rustc.
    command = ["cargo", "test", "-p", "deadpan-media-worker", "--locked",
               "--target", "aarch64-apple-darwin", "--target-dir", str(work / "target")]
    start = time.monotonic()
    with (work / "test.log").open("w") as log:
        exit_code, failure = run_test_command(command, cwd=repo, env=env, log=log)
    binary = work / "target/aarch64-apple-darwin/debug/deadpan-media-worker"
    report = {"scope": "worker native C and target C dependencies ASan/UBSan; Rust and FFmpeg libraries not instrumented",
              "command": command, "cflags": env["CFLAGS"], "rust_flags": flags,
              "runtime": str(runtime), "runtime_sha256": hashlib.sha256(runtime.read_bytes()).hexdigest(),
              "ffmpeg_prefix": prefix, "elapsed_seconds": time.monotonic() - start,
              "exit_code": exit_code, "failure": failure,
              "test_log_sha256": hashlib.sha256((work / "test.log").read_bytes()).hexdigest(),
              "worker": str(binary),
              "worker_sha256": hashlib.sha256(binary.read_bytes()).hexdigest() if binary.exists() else None}
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2), flush=True)
    if exit_code or failure:
        print((work / "test.log").read_text()[-16000:], flush=True)
        raise SystemExit(exit_code if exit_code > 0 else 1)


if __name__ == "__main__":
    main()
