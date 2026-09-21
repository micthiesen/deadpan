#!/usr/bin/env python3
"""Measure a shared canonical DSP schedule and exact replay/prepared-PCM seeks."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

from canonical_analysis import analyze
from run import ROOT, command, download, sha256


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--download-cache", type=Path)
    parser.add_argument("--sanitizers", action="store_true")
    parser.add_argument("--window", choices=("120-30", "120-15", "60-15"), default="120-15",
                        help="DSP analysis-window and hop milliseconds; retain failed alternatives")
    args = parser.parse_args()
    if sys.version_info < (3, 12):
        parser.error("Python 3.12+ is required for safe archive extraction")
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("This measured build configuration requires Apple Silicon macOS")
    work = Path(tempfile.mkdtemp(prefix="deadpan-canonical-audio-", dir="/tmp"))
    report = {
        "schema_version": 1,
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "runner_argv": sys.argv,
        "work_directory": str(work),
        "sanitizers": args.sanitizers,
        "analysis_window_ms": args.window,
        "python_version": sys.version,
        "platform": platform.platform(),
        "harness_sha256": {
            name: sha256(ROOT / name)
            for name in (
                "canonical_run.py", "canonical.hpp", "canonical_probe.cpp",
                "canonical_analysis.py", "test_canonical_analysis.py",
                "run.py", "probe.cpp", "pins.json",
            )
        },
        "commands": [],
        "status": "incomplete",
    }
    report["harness_sha256"]["native/deadpan-dsp/src/canonical.hpp"] = sha256(
        ROOT.parent.parent / "native/deadpan-dsp/src/canonical.hpp"
    )
    code = 2
    try:
        commands = report["commands"]
        report["host"] = {
            name: command(argv, commands)
            for name, argv in {
                "os": ["sw_vers"], "cpu": ["sysctl", "-n", "machdep.cpu.brand_string"],
                "memory_bytes": ["sysctl", "-n", "hw.memsize"], "machine_model": ["sysctl", "-n", "hw.model"],
                "compiler": ["clang++", "--version"], "sdk_version": ["xcrun", "--show-sdk-version"],
                "sdk_path": ["xcrun", "--show-sdk-path"],
            }.items()
        }
        pins = json.loads((ROOT / "pins.json").read_text())
        sources = {name: download(name, pin, work, args.download_cache) for name, pin in pins.items()}
        report["sources"] = {
            name: {
                **pins[name], "path": str(path),
                "license_text": (path / "LICENSE.txt").read_text(),
                "license_sha256": sha256(path / "LICENSE.txt"),
                "header_sha256": {str(header.relative_to(path)): sha256(header) for header in sorted(path.rglob("*.h"))},
            } for name, path in sources.items()
        }
        executable = work / "canonical_audio_probe"
        flags = ["-std=c++17", "-O2", "-g", "-Wall", "-Wextra", "-Werror", "-arch", "arm64", "-mmacosx-version-min=15.0"]
        if args.sanitizers:
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-fno-sanitize-recover=all"]
        includes = [argument for source in sources.values() for argument in ("-isystem", str(source / "include"))]
        command(["clang++", *flags, *includes, str(ROOT / "canonical_probe.cpp"), "-o", str(executable)], commands, 180)
        report["build"] = {
            "flags": flags, "fft_backend": "Signalsmith Linear portable built-in FFT",
            "binary_sha256": sha256(executable),
            "dynamic_libraries": command(["otool", "-L", str(executable)], commands),
            "load_commands": command(["otool", "-l", str(executable)], commands),
        }
        pcm = work / "pcm"
        pcm.mkdir()
        print(f"Running canonical probe; retained PCM: {pcm}", flush=True)
        native_text = command([str(executable), str(pcm), args.window], commands, 300)
        (work / "native.json").write_text(native_text + "\n")
        native = json.loads(native_text)
        report["native"] = native
        report["analysis"] = analyze(native, pcm, args.window)
        failed = report["analysis"]["assertions_failed"]
        report["status"] = "adapter_targets_failed" if failed else "adapter_targets_passed"
        code = int(bool(failed))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        report["status"] = "harness_error"
        report["error"] = f"{type(error).__name__}: {error}"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(json.dumps({
        "report": str(args.output), "status": report["status"], "exit_code": code,
        "passed": report.get("analysis", {}).get("assertions_passed"),
        "failed": report.get("analysis", {}).get("assertions_failed"), "error": report.get("error"),
    }, indent=2))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
