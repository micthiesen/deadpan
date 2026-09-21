#!/usr/bin/env python3
"""Build pinned upstream candidates in scratch; never alter the app dependencies.

Pass an existing native run.py report to compare Cutlass with its exact fixtures.
Qualification failures are preserved in JSON and return a nonzero exit code.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parent
PINS = {
    "cutlass": ("https://github.com/1mrnewton/cutlass.git", "22437e2837340c7c57d62e438117f9a0fb4096d2"),
    "rsmpeg": ("https://github.com/larksuite/rsmpeg.git", "b21fcfde8bb1ffdc179504e370e330385baa9819"),
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--media-report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--existing-clones", type=Path, help="prefix containing PIN-verified cutlass/ and rsmpeg/ clones")
    args = parser.parse_args()
    media_report_bytes = args.media_report.read_bytes()
    native = json.loads(media_report_bytes)
    work = Path(tempfile.mkdtemp(prefix="deadpan-media-upstream-", dir="/tmp"))
    input_names = ("audit_upstream.py", "cutlass_probe.rs", "cutlass-probe.Cargo.toml",
                   "cutlass-probe.Cargo.lock", "rsmpeg.Cargo.lock")
    report: dict = {
        "source_pins": PINS, "work_directory": str(work), "commands": [], "failures": [], "licenses": {},
        "inputs_sha256": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in input_names},
        "media_report_sha256": hashlib.sha256(media_report_bytes).hexdigest(),
        "source_checkout_checks": {},
    }

    def run(argv, cwd=work, required=True, timeout=300):
        start = time.monotonic()
        result = subprocess.run(list(map(str, argv)), cwd=cwd, capture_output=True, text=True, timeout=timeout)
        report["commands"].append({"argv": list(map(str, argv)), "cwd": str(cwd), "exit_code": result.returncode,
                                   "elapsed_seconds": time.monotonic() - start, "stdout": result.stdout, "stderr": result.stderr})
        if result.returncode != 0:
            report["failures"].append({"argv": list(map(str, argv)), "exit_code": result.returncode})
            if required:
                raise RuntimeError(f"command failed: {argv[0]} ({result.returncode})")
        return result

    try:
        paths = {}
        for name, (url, revision) in PINS.items():
            path = args.existing_clones / name if args.existing_clones else work / f"{name}-git"
            if args.existing_clones is None:
                run(["git", "init", path])
                run(["git", "-c", "http.userAgent=OpenAI File Downloader, XaiImageApiFetch/1.0",
                     "fetch", "--depth=1", url, revision], cwd=path)
                run(["git", "checkout", "--detach", "FETCH_HEAD"], cwd=path)
            actual = run(["git", "rev-parse", "HEAD"], cwd=path).stdout.strip()
            if actual != revision:
                raise RuntimeError(f"{name}: clone does not match exact audited revision")
            # Reject staged, unstaged, and untracked additions. Ignored build
            # output is reported separately, and never enters the build snapshot.
            dirty = run(["git", "status", "--porcelain=v1", "--untracked-files=all"], cwd=path).stdout
            ignored = run(["git", "ls-files", "--others", "--ignored", "--exclude-standard", "--directory"], cwd=path).stdout
            report["source_checkout_checks"][name] = {
                "head": actual, "tracked_or_untracked_changes": dirty, "ignored_paths_excluded": ignored.splitlines(),
            }
            if dirty:
                raise RuntimeError(f"{name}: tracked or untracked changes in upstream checkout")
            # Compile only committed files. This excludes ignored Cargo config,
            # generated sources, and changing build output from supplied clones.
            archive = work / f"{name}.tar"
            run(["git", "archive", "--format=tar", "--output", archive, revision], cwd=path)
            snapshot = work / name
            snapshot.mkdir()
            with tarfile.open(archive) as source:
                source.extractall(snapshot, filter="data")
            paths[name] = snapshot
            licenses = {}
            for file in snapshot.glob("LICENSE*"):
                licenses[file.name] = {"sha256": hashlib.sha256(file.read_bytes()).hexdigest(), "text": file.read_text()}
            report["licenses"][name] = licenses
        run(["cargo", "test", "-p", "cutlass-decoder", "-p", "cutlass-encoder", "--locked", "--no-default-features"],
            cwd=paths["cutlass"], required=False)
        shutil.copyfile(ROOT / "rsmpeg.Cargo.lock", paths["rsmpeg"] / "Cargo.lock")
        run(["cargo", "check", "--locked", "--lib", "--features", "link_system_ffmpeg"], cwd=paths["rsmpeg"], required=False)
        report["rsmpeg_resolved_lock"] = (paths["rsmpeg"] / "Cargo.lock").read_text()
        report["cutlass_lock_sha256"] = hashlib.sha256((paths["cutlass"] / "Cargo.lock").read_bytes()).hexdigest()
        downstream = work / "comparison"
        (downstream / "src").mkdir(parents=True)
        shutil.copyfile(ROOT / "cutlass-probe.Cargo.toml", downstream / "Cargo.toml")
        shutil.copyfile(ROOT / "cutlass-probe.Cargo.lock", downstream / "Cargo.lock")
        shutil.copyfile(ROOT / "cutlass_probe.rs", downstream / "src/main.rs")
        report["downstream_inputs_sha256"] = {
            name: hashlib.sha256((downstream / name).read_bytes()).hexdigest() for name in ("Cargo.toml", "Cargo.lock", "src/main.rs")
        }
        run(["cargo", "build", "--release", "--locked"], cwd=downstream)
        comparisons = []
        for case in native["cases"]:
            if case["status"] != "passed scoped assertions":
                continue
            fixture = Path(native["work_directory"]) / f"{case['name']}.mp4"
            result = run([downstream / "target/release/deadpan-cutlass-qualification", fixture, case["fixture"]["cadence"]],
                         required=False)
            comparisons.append({"name": case["name"], "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
                                "exit_code": result.returncode,
                                "metrics": json.loads(result.stdout) if result.returncode == 0 else None})
        report["comparisons"] = comparisons
        report["downstream_resolved_lock"] = (downstream / "Cargo.lock").read_text()
    except (RuntimeError, OSError, subprocess.TimeoutExpired, ValueError) as error:
        report["failures"].append({"harness_error": str(error)})
    finally:
        report["status"] = "failed qualification" if report["failures"] else "passed scoped assertions"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        print(f"{report['status']}; report: {args.output}; scratch: {work}")
    return 1 if report["failures"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
