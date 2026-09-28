#!/usr/bin/env python3
"""Measure a bounded SDR MP4 encoder matrix; never claim product export acceptance."""

from __future__ import annotations

import argparse
import array
from datetime import datetime, timezone
import importlib.util
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
import sys
import time

from mp4_boxes import inspect_mp4
from encoder_oracle import CaseSpec, inspect_case, movie_timescale

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("native_qualification", ROOT.parent / "run.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class EncoderHarness(native.Harness):
    """Keep every command's complete output and every independently failed case."""

    def __init__(self, work: Path, sanitizers: bool, build_report: Path):
        super().__init__(work, sanitizers)
        self.build = json.loads(build_report.read_text())
        self.prefix = Path(self.build["prefix"]).resolve()
        self.binary = work / "export_probe"
        self.admitted_files = {}
        self.loaded_paths = {}
        self.process_faults = []
        self.environment = dict(os.environ, PATH=str(self.prefix / "bin") + os.pathsep + os.environ["PATH"],
                                PKG_CONFIG_PATH="", PKG_CONFIG_LIBDIR=str(self.prefix / "lib/pkgconfig"),
                                MACOSX_DEPLOYMENT_TARGET="15.0")
        removed = sorted(name for name in self.environment if name.startswith(("DYLD_", "LD_")))
        for name in removed:
            del self.environment[name]
        if sanitizers:
            self.environment.update(ASAN_OPTIONS="halt_on_error=1:abort_on_error=1:exitcode=86:detect_leaks=0",
                                    UBSAN_OPTIONS="halt_on_error=1:abort_on_error=1:exitcode=87:print_stacktrace=1")
        self.report.update(scope="developer SDR encoder/mux experiment, not app integration or export acceptance",
                           result="failed", build_report={"path": str(build_report), "sha256": native.digest(build_report)},
                           prefix=str(self.prefix), removed_loader_override_names=removed,
                           negative_controls=[], unqualified=[])
        self.report["process_faults"] = self.process_faults
        self.report["sanitizer_options"] = {key: self.environment[key] for key in ("ASAN_OPTIONS", "UBSAN_OPTIONS")
                                             if key in self.environment}
        self.report["source_sha256_at_start"] = source_inventory()
        self.admitted_files[str(build_report)] = self.report["build_report"]["sha256"]

    def run(self, argv, *, required=True, timeout=120):
        argv = [str(value) for value in argv]
        number = len(self.report["commands"])
        paths = {name: self.work / f"command-{number:03d}.{name}" for name in ("stdout", "stderr")}
        record = {"argv": argv, "cwd": str(self.work), "timeout_seconds": timeout}
        self.report["commands"].append(record)
        start = time.monotonic()
        timed_out = False
        try:
            with paths["stdout"].open("wb") as stdout, paths["stderr"].open("wb") as stderr:
                try:
                    result = subprocess.run(argv, cwd=self.work, env=self.environment, stdout=stdout,
                                            stderr=stderr, check=False, timeout=timeout)
                    record["exit_code"] = result.returncode
                except subprocess.TimeoutExpired:
                    timed_out = True
                    record["timed_out"] = True
                    record["exit_code"] = None
                    self.process_faults.append({"command": number, "reason": "timeout"})
                except OSError as error:
                    record["exit_code"] = None
                    record["launch_error"] = str(error)
                    self.process_faults.append({"command": number, "reason": "launch error"})
                    raise RuntimeError(f"command {number} could not launch: {error}") from error
        finally:
            record["elapsed_seconds"] = time.monotonic() - start
            record["logs"] = {name: {"path": str(path), "bytes": path.stat().st_size,
                                      "sha256": native.digest(path)} for name, path in paths.items() if path.exists()}
        if timed_out:
            raise RuntimeError(f"command {number} timed out; retained logs: {paths['stderr']}")
        # These tools inspect at most 240 generated frames. Reject unexpectedly
        # large output before loading it; original log files remain available.
        if any(path.stat().st_size > 16 * 1024 * 1024 for path in paths.values()):
            self.process_faults.append({"command": number, "reason": "output limit"})
            raise RuntimeError(f"command {number} exceeded the 16 MiB report-read limit")
        result = subprocess.CompletedProcess(argv, record["exit_code"],
                                             paths["stdout"].read_text(), paths["stderr"].read_text())
        if result.returncode < 0 or result.returncode in (86, 87) or any(
            marker in result.stderr for marker in ("ERROR: AddressSanitizer", "UndefinedBehaviorSanitizer", "runtime error:", "ERROR: LeakSanitizer")
        ):
            self.process_faults.append({"command": number, "reason": "signal or sanitizer failure"})
        if required and result.returncode != 0:
            raise RuntimeError(f"command {number} failed ({result.returncode}); retained logs: {paths['stderr']}")
        return result

    def prepare(self):
        self.assert_that("successful pinned build report", self.build["result"] == "passed")
        pins = json.loads((ROOT / "pins.json").read_text())
        self.assert_that("build report uses current FFmpeg pin", self.build["pins"]["ffmpeg"] == pins["ffmpeg"])
        self.assert_that("build commands completed successfully", bool(self.build["commands"])
                         and all(command.get("exit_code") == 0 for command in self.build["commands"]))
        self.assert_that("build records pinned release signature", f"[GNUPG:] VALIDSIG {pins['ffmpeg']['signing_key_fingerprint']} "
                         in self.build["signature_verification"])
        self.assert_that("LGPL-only offline build configuration", all(flag in self.build["configure_argv"] for flag in
                         ("--disable-gpl", "--disable-nonfree", "--disable-version3", "--disable-network", "--disable-autodetect")))
        self.assert_that("Apple Silicon macOS", platform.system() == "Darwin" and platform.machine() == "arm64")
        self.report["host"] = {"platform": platform.platform(),
                               "processor": self.run(["sysctl", "-n", "machdep.cpu.brand_string"]).stdout.strip(),
                               "memory_bytes": int(self.run(["sysctl", "-n", "hw.memsize"]).stdout),
                               "macos": self.run(["sw_vers"]).stdout.strip(),
                               "sdk": self.run(["xcrun", "--show-sdk-version"]).stdout.strip(),
                               "compiler": self.run(["clang", "--version"]).stdout}
        self.report["libraries"] = {}
        for name, library in self.build["libraries"].items():
            path = Path(library["path"]).resolve()
            self.assert_that(f"{name}: admitted prefix", path.parent == self.prefix / "lib")
            self.assert_that(f"{name}: recorded library bytes", native.digest(path) == library["sha256"])
            self.admitted_files[str(path)] = library["sha256"]
            linked = self.run(["otool", "-L", path]).stdout
            version = self.run(["vtool", "-show-build", path]).stdout
            self.assert_that(f"{name}: deployment and linkage", "minos 15.0" in version and "/opt/homebrew" not in linked)
            self.report["libraries"][name] = {"path": str(path), "sha256": native.digest(path),
                                               "linked_libraries": linked, "build_version": version}
        self.assert_that("codec/format/util libraries admitted", all(any(name.startswith(library + ".")
                          for name in self.report["libraries"]) for library in ("libavcodec", "libavformat", "libavutil")))
        for library in self.report["libraries"].values():
            self.admit_linkage(library["linked_libraries"])
        self.report["headers"] = {str(path): native.digest(path) for path in sorted((self.prefix / "include").rglob("*.h"))}
        self.assert_that("installed headers are present", bool(self.report["headers"]))
        self.admitted_files.update(self.report["headers"])
        libdir = Path(self.run(["pkg-config", "--variable=libdir", "libavcodec"]).stdout.strip()).resolve()
        self.assert_that("isolated pkg-config library directory", libdir == self.prefix / "lib")
        flags = shlex.split(self.run(["pkg-config", "--cflags", "--libs", "libavformat", "libavcodec", "libavutil"]).stdout)
        self.assert_that("pkg-config includes admitted headers", any(flag.startswith("-I") and
                         Path(flag[2:]).resolve() == self.prefix / "include" for flag in flags))
        flags += ["-arch", "arm64", "-mmacosx-version-min=15.0"]
        if self.sanitizers:
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
        self.run(["clang", "-std=c11", "-O2", "-g", "-Wall", "-Wextra", "-Werror",
                  ROOT / "export_probe.c", "-o", self.binary, *flags])
        self.report["native"] = json.loads(self.run([self.binary, "inventory"]).stdout)
        self.assert_that("pinned runtime and LGPL license", self.report["native"]["ffmpeg"] == pins["ffmpeg"]["version"]
                         and self.report["native"]["license"] == "LGPL version 2.1 or later")
        self.report["binary"] = {"path": str(self.binary), "sha256": native.digest(self.binary),
                                 "linked_libraries": self.run(["otool", "-L", self.binary]).stdout}
        self.admit_linkage(self.report["binary"]["linked_libraries"])
        self.assert_that("probe links admitted codec and format", all(str(self.prefix / "lib" / name)
                         in self.report["binary"]["linked_libraries"] for name in ("libavcodec.62.dylib", "libavformat.62.dylib")))
        self.report["ffprobe"] = {"path": str(self.prefix / "bin/ffprobe"),
                                  "sha256": native.digest(self.prefix / "bin/ffprobe"),
                                  "version": self.run(["ffprobe", "-version"]).stdout,
                                  "linked_libraries": self.run(["otool", "-L", self.prefix / "bin/ffprobe"]).stdout}
        self.admit_linkage(self.report["ffprobe"]["linked_libraries"])
        self.admitted_files[str(self.binary)] = self.report["binary"]["sha256"]
        self.admitted_files[str(self.prefix / "bin/ffprobe")] = self.report["ffprobe"]["sha256"]
        self.assert_that("ffprobe reports pinned version", self.report["ffprobe"]["version"].startswith("ffprobe version " + pins["ffmpeg"]["version"] + " "))
        self.assert_that("ffprobe links admitted codec and format", all(str(self.prefix / "lib" / name)
                         in self.report["ffprobe"]["linked_libraries"] for name in ("libavcodec.62.dylib", "libavformat.62.dylib")))

    def admit_linkage(self, output):
        for line in output.splitlines()[1:]:
            name = line.strip().split(" (", 1)[0]
            if not Path(name).name.startswith(("libav", "libswresample.", "libswscale.")):
                continue
            path = Path(name)
            resolved = path.resolve()
            self.assert_that(f"admitted FFmpeg dependency: {name}", path.is_absolute()
                             and str(resolved) in self.admitted_files and resolved.parent == self.prefix / "lib")
            actual = native.digest(path)
            self.assert_that(f"load-path bytes: {name}", actual == self.admitted_files[str(resolved)])
            self.loaded_paths[name] = {"resolved": str(resolved), "sha256": actual}
        self.report["loaded_paths"] = self.loaded_paths

    def finish_admission(self):
        observations = []
        for name, expected in self.admitted_files.items():
            try:
                actual = native.digest(Path(name))
                observations.append({"path": name, "expected": expected, "actual": actual, "passed": actual == expected})
            except OSError as error:
                observations.append({"path": name, "passed": False, "error": str(error)})
        self.report["final_file_admission"] = observations
        for name, expected in self.loaded_paths.items():
            try:
                actual = {"resolved": str(Path(name).resolve()), "sha256": native.digest(Path(name))}
                observations.append({"load_path": name, "expected": expected, "actual": actual, "passed": actual == expected})
            except OSError as error:
                observations.append({"load_path": name, "passed": False, "error": str(error)})
        if any(not row["passed"] for row in observations):
            self.report["result"] = "failed: admitted executable or library changed"

    def artifact(self, path):
        return {"path": str(path), "sha256": native.digest(path), "bytes": path.stat().st_size}

    def capture_case(self, name, mode, edits, fps=(30000, 1001), count=120, pcm_kind="impulses"):
        case = {"name": name, "requested": {"mode": mode, "edit_lists": edits, "fps": list(fps),
                                             "frames": count, "pcm": pcm_kind}, "status": "failed", "checks": []}
        self.report["cases"].append(case)
        path = self.work / f"{name}.mp4"
        packet_log = self.work / f"{name}.before-mux.jsonl"
        try:
            encoded = self.run([self.binary, "encode", path, packet_log, mode, edits, *fps, count, pcm_kind], required=False)
            case["encode_exit_code"] = encoded.returncode
            if encoded.returncode:
                case["failure"] = "encoder or mux rejected the explicit requested configuration; see command logs"
                case["known_capability_rejection"] = (encoded.returncode == 1
                    and "pts (" in encoded.stderr and " < dts (" in encoded.stderr)
                return case
            case["source"] = json.loads(encoded.stdout)
            case["boxes"] = inspect_mp4(path)
            case["video"] = json.loads(self.run([self.binary, "video", path]).stdout)
            case["packets"] = json.loads(self.run([self.binary, "packets", path]).stdout)
            case["ffprobe"] = json.loads(self.run(["ffprobe", "-v", "error", "-show_streams", "-show_format",
                                                   "-print_format", "json", path]).stdout)
            case["audio"] = {}
            for decoder in ("ordinary", "manual"):
                pcm = self.work / f"{name}.{decoder}.f32"
                case["audio"][decoder] = json.loads(self.run([self.binary, "audio", path, pcm, decoder]).stdout)
                case["audio"][decoder]["pcm"] = self.artifact(pcm)
            case["status"] = "captured, acceptance not evaluated"
        except (ValueError, OSError, RuntimeError) as error:
            case["failure"] = str(error)
        finally:
            case["artifacts"] = {key: self.artifact(value) for key, value in
                                  (("mp4", path), ("before_mux", packet_log)) if value.is_file()}
        return case


def read_pcm(path):
    if path.stat().st_size > 64 * 1024 * 1024 or path.stat().st_size % 8:
        raise ValueError("PCM is not bounded interleaved stereo f32")
    result = array.array("f")
    result.frombytes(path.read_bytes())
    if sys.byteorder != "little":
        result.byteswap()
    return result


def source_inventory():
    return {str(path.relative_to(ROOT.parent)): native.digest(path)
            for path in [ROOT.parent / "run.py", ROOT.parent / "media_probe.c",
                         *sorted(ROOT.glob("*.py")), ROOT / "export_probe.c", ROOT / "media_probe.c", ROOT / "pins.json"] if path.is_file()}


def evaluate_case(case):
    if case["status"] != "captured, acceptance not evaluated":
        return
    requested = case["requested"]
    num, den = requested["fps"]
    definition = CaseSpec(frame_count=requested["frames"], fps_num=num, fps_den=den,
                          pcm_kind=requested["pcm"], requested_b_frames=0 if requested["mode"].endswith("no-b") else 2)
    # Record the maximum integral error still strictly below an output frame.
    # The oracle separately reports exact-sample equality and measured errors.
    tolerance = (48000 * den + num - 1) // num - 1
    case["declared_encoded_tolerance_samples"] = tolerance
    try:
        ordinary = case["audio"]["ordinary"]
        manual = case["audio"]["manual"]
        result = inspect_case(definition, case["source"], case["video"], ordinary, manual, case["ffprobe"],
                              ordinary_pcm=read_pcm(Path(ordinary["pcm"]["path"])),
                              manual_pcm=read_pcm(Path(manual["pcm"]["path"])),
                              tolerance_samples=tolerance, packets=case["packets"])
        case["checks"] = result["checks"]
        case["observations"] = result["observations"]
        case["scope"] = result.get("scope")
        case["unqualified"] = result.get("unqualified", [])
        boxes = case["boxes"]
        checks = case["checks"]
        checks.append({"label": "one fast-start movie and two tracks", "passed":
                       boxes["fast_start"]["moov_count"] == 1 and boxes["fast_start"]["mdat_count"] >= 1
                       and boxes["fast_start"]["moov_before_mdat"] is True
                       and sum(box["path"] == ["moov", "trak"] for box in boxes["boxes"]) == 2})
        movie = [value for value in boxes["timescales"] if value["path"] == ["moov", "mvhd"]]
        checks.append({"label": "exact movie timescale", "passed": len(movie) == 1
                       and movie[0]["timescale"] == movie_timescale(num, den), "details": movie})
        if requested["edit_lists"] == "disabled":
            checks.append({"label": "no edit list or edit container", "passed": not boxes["has_elst"]
                           and not boxes["has_edts"]})
        before_path = Path(case["artifacts"]["before_mux"]["path"])
        if before_path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("before-mux report exceeds read limit")
        before = [json.loads(line) for line in before_path.read_text().splitlines()]
        after = case["packets"]["packets"]
        case["packet_payloads"] = {}
        for codec in ("video", "audio"):
            left = [packet["sha256"] for packet in before if packet["codec_type"] == codec]
            right = [packet["sha256"] for packet in after if packet["codec_type"] == codec]
            case["packet_payloads"][codec] = right
            checks.append({"label": f"{codec}: raw packet payload equality across mux", "passed": bool(left) and left == right,
                           "diagnostic": codec == "video",
                           "details": {"before_count": len(left), "after_count": len(right),
                                       "scope": "H.264 mux framing may change; raw video bytes are observational. AAC equality is required."}})
        case["scoped_checks_passed"] = all(check["passed"] for check in checks if not check.get("diagnostic", False))
        case["status"] = "passed scoped checks" if case["scoped_checks_passed"] else "failed scoped checks"
    except (ValueError, OSError, KeyError, TypeError) as error:
        case["failure"] = str(error)
        case["status"] = "observation could not be evaluated"
        case["scoped_checks_passed"] = False


def compare_edge_reference(reference, candidate):
    """Observe unshifted edge content only; this is not an AAC quality metric."""
    if "packet_payloads" not in reference or "packet_payloads" not in candidate:
        return {"measured": False, "reason": "one case lacks complete packet/PCM observations"}
    same = reference["packet_payloads"]["audio"] == candidate["packet_payloads"]["audio"]
    result = {"measured": same, "same_encoded_aac_payloads": same,
              "scope": "absolute-coordinate comparison, without shifting, cropping or correcting either file"}
    if not same:
        result["reason"] = "separate encoder invocations emitted different AAC payloads"
        return result
    total = reference["source"]["audio_samples"]
    inputs = [value["audio"]["ordinary"] for value in (reference, candidate)]
    pcm = [read_pcm(Path(value["pcm"]["path"])) for value in inputs]
    first = [value["first_sample_pts"] for value in inputs]
    if any(type(value) is not int for value in first):
        return {**result, "measured": False, "reason": "missing integer first sample coordinate"}
    windows = ((0, min(2048, total)), (max(0, total - 2048), total))
    result["windows"] = []
    for start, end in windows:
        differences = []
        missing = 0
        for sample in range(start, end):
            positions = [sample - origin for origin in first]
            if any(index < 0 or index >= len(values) // 2 for index, values in zip(positions, pcm)):
                missing += 1
                continue
            differences.extend(abs(pcm[0][positions[0] * 2 + channel] - pcm[1][positions[1] * 2 + channel])
                               for channel in (0, 1))
        result["windows"].append({"start_sample": start, "end_sample": end, "missing_frames": missing,
                                   "maximum_absolute_difference": max(differences, default=None),
                                   "equal_at_authored_coordinates": missing == 0 and all(value == 0 for value in differences)})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-report", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--sanitizers", action="store_true")
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()):
        parser.error("work directory must be empty")
    harness = EncoderHarness(work, args.sanitizers, args.build_report.resolve())
    try:
        harness.prepare()
        for name, mode, edits in (
            ("hardware-default-edit-list", "hardware-no-b", "default"),
            ("hardware-no-b", "hardware-no-b", "disabled"),
            ("software-no-b", "software-no-b", "disabled"),
            ("software-b", "software-b", "disabled"),
            ("hardware-b", "hardware-b", "disabled"),
        ):
            print(f"Measuring {name}...", flush=True)
            evaluate_case(harness.capture_case(name, mode, edits))
        reference = harness.report["cases"][0]
        rejected = reference.get("boxes", {}).get("has_elst") is True
        harness.report["negative_controls"].append({"name": "default edit-list file rejected by no-edit-list rule",
                                                     "passed": rejected})
        # Prefer hardware. Continue a captured path diagnostically even if its
        # first timing checks fail; otherwise a low-rate fixture could hide the
        # decisive 60 fps bound. No silent encoder fallback occurs within a case.
        selected = next((case for case in harness.report["cases"][1:]
                         if case["name"] in ("hardware-no-b", "software-no-b") and "observations" in case), None)
        if selected:
            mode = selected["requested"]["mode"]
            harness.report["selected_diagnostic_path"] = mode
            for label, fps, count, pcm in (("edge-2997", (30000, 1001), 120, "edges"),
                                          ("impulse-60", (60, 1), 120, "impulses"),
                                          ("edge-one-60", (60, 1), 1, "edges"),
                                          ("edge-25", (25, 1), 32, "edges")):
                pair = []
                for edits in ("default", "disabled"):
                    print(f"Measuring {label}/{edits}...", flush=True)
                    case = harness.capture_case(f"{label}-{edits}", mode, edits, fps, count, pcm)
                    evaluate_case(case)
                    pair.append(case)
                if pcm == "edges":
                    pair[1]["absolute_edge_reference_comparison"] = compare_edge_reference(*pair)
        candidates = [case for case in harness.report["cases"] if case["requested"]["edit_lists"] == "disabled"]
        harness.report["unqualified"] = ["fresh-decoder closed GOP independence", "second native decoder/player stack",
                                         "full-resolution quality/performance", "renderer-to-Rec.709 encoder transform",
                                         "complete authored boundary-content and audible quality acceptance", "product export integration"]
        harness.report["experiment_completed"] = True
        harness.report["result"] = ("passed scoped encoder checks" if rejected and selected
                                    and all(case.get("scoped_checks_passed", False) for case in harness.report["cases"]
                                            if case["requested"]["edit_lists"] == "default")
                                    and all(case.get("scoped_checks_passed", False) for case in candidates
                                            if case["requested"]["mode"] == selected["requested"]["mode"])
                                    else "completed: selected path remains unqualified by this timing matrix")
        unexpected = [case["name"] for case in harness.report["cases"] if
                      case["status"] in ("failed", "observation could not be evaluated")
                      and not case.get("known_capability_rejection", False)]
        harness.report["unexpected_case_failures"] = unexpected
        if harness.process_faults or unexpected:
            harness.report["result"] = "failed: process or observation failure"
    finally:
        harness.finish_admission()
        harness.report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        harness.report["source_sha256"] = source_inventory()
        harness.report["source_unchanged_during_run"] = harness.report["source_sha256"] == harness.report["source_sha256_at_start"]
        if not harness.report["source_unchanged_during_run"]:
            harness.report["result"] = "failed: source changed during the run"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(harness.report, indent=2) + "\n")
        print(json.dumps({"result": harness.report["result"], "report": str(args.output),
                          "cases": len(harness.report["cases"])}), flush=True)
    return 0 if harness.report["result"] == "passed scoped encoder checks" else 1


if __name__ == "__main__":
    raise SystemExit(main())
