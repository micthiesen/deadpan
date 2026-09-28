#!/usr/bin/env python3
"""Read the retained 120-frame/60-fps encoder pair with AVFoundation, without encoding."""

from __future__ import annotations

import argparse
import array
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import stat
import sys

from encoder_oracle import CaseSpec
from native_audio_oracle import inspect_native_audio
from recorded_harness import RecordedHarness, digest


ROOT = Path(__file__).resolve().parent
MAX_INPUT = 64 * 1024 * 1024
MAX_PCM = 192000 * 8
CASE_NAMES = ("impulse-60-default", "impulse-60-disabled")
SOURCE_NAMES = ("avfoundation_probe.m", "qualify_native_audio.py", "recorded_harness.py",
                "native_audio_oracle.py", "encoder_oracle.py", "test_native_audio_runner.py",
                "test_native_audio_oracle.py")
RUNTIME_NAMES = {"libclang_rt.asan_osx_dynamic.dylib", "libclang_rt.ubsan_osx_dynamic.dylib"}
PASS_RESULT = "passed scoped AVFoundation reader timing"


def source_inventory():
    return {name: digest(ROOT / name) for name in SOURCE_NAMES}


def bounded_file(path, maximum, *, allow_empty=False):
    """Admit regular bytes without following a final symlink or blocking on a FIFO."""
    path = Path(path)
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or not (0 if allow_empty else 1) <= before.st_size <= maximum:
            raise ValueError(f"not a bounded regular file: {path}")
        chunks, remaining = [], maximum + 1
        while remaining:
            chunk = os.read(descriptor, min(65536, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        data = b"".join(chunks)
        after, named = os.fstat(descriptor), os.stat(path, follow_symlinks=False)
        identity = lambda info: (info.st_dev, info.st_ino, info.st_mode, info.st_size,
                                 info.st_mtime_ns, info.st_ctime_ns)
        if len(data) != before.st_size or identity(before) != identity(after) or identity(after) != identity(named):
            raise ValueError(f"file changed during bounded admission: {path}")
        return data, {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    finally:
        os.close(descriptor)


def strict_json(data):
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def constant(value):
        raise ValueError(f"nonfinite JSON value: {value}")

    def finite(value):
        number = float(value)
        if not math.isfinite(number):
            raise ValueError("nonfinite JSON number")
        return number

    try:
        return json.loads(data, object_pairs_hook=pairs, parse_constant=constant, parse_float=finite)
    except RecursionError as error:
        raise ValueError("excessive JSON nesting") from error


def exact(value, expected):
    return type(value) is type(expected) and value == expected


def require_fixture(case, name):
    if not isinstance(case, dict) or case.get("name") != name or not exact(case.get("encode_exit_code"), 0):
        raise ValueError("missing or unsuccessful recorded encoder case")
    edits = name.removeprefix("impulse-60-")
    requested, source = case.get("requested"), case.get("source")
    if not isinstance(requested, dict) or not isinstance(source, dict):
        raise ValueError("missing requested parameters or encoder source manifest")
    for key, expected in {"edit_lists": edits, "frames": 120, "pcm": "impulses"}.items():
        if not exact(requested.get(key), expected):
            raise ValueError(f"wrong requested {key}")
    for key, expected in {"schema_version": 1, "kind": "encode", "edit_lists": edits,
                          "frame_count": 120, "pcm_kind": "impulses", "audio_samples": 96000,
                          "audio_offset_samples": 0, "start_pts": 0, "duration_ticks": 120,
                          "width": 320, "height": 180, "requested_b_frames": 0}.items():
        if not exact(source.get(key), expected):
            raise ValueError(f"wrong encoder source {key}")
    for value, expected in ((requested.get("fps"), [60, 1]), (source.get("frame_rate"), [60, 1]),
                            (source.get("time_base"), [1, 60]), (source.get("impulses"), [100, 48000, 95800])):
        if not isinstance(value, list) or len(value) != len(expected) or any(not exact(a, b) for a, b in zip(value, expected)):
            raise ValueError("wrong exact fixture clock or impulse positions")
    if requested.get("mode") not in ("hardware-no-b", "software-no-b") or source.get("mode") != requested["mode"]:
        raise ValueError("unexpected recorded encoder mode")
    return source


def dependencies(output):
    lines = output.splitlines()
    if not lines or not lines[0].endswith(":"):
        raise ValueError("unrecognized otool linkage report")
    result = []
    for line in lines[1:]:
        if not line.strip():
            continue
        if " (compatibility version " not in line:
            raise ValueError("unrecognized linked dependency")
        result.append(line.strip().split(" (", 1)[0])
    if not result or len(result) > 128:
        raise ValueError("empty or excessive linked dependency inventory")
    return result


def rpaths(output):
    result, waiting = [], False
    for line in output.splitlines():
        text = line.strip()
        if text.startswith("cmd "):
            waiting = text == "cmd LC_RPATH"
        elif waiting and text.startswith("path ") and " (offset " in text:
            result.append(text[5:].split(" (offset ", 1)[0])
            waiting = False
    if len(result) > 32:
        raise ValueError("excessive runtime search paths")
    return result


def system_dependency(name):
    return (name.startswith(("/System/Library/", "/usr/lib/"))
            and ".." not in Path(name).parts and str(Path(name)) == name)


class NativeAudioHarness(RecordedHarness):
    def __init__(self, work, sanitizers, encoder_report):
        super().__init__(work, sanitizers)
        self.encoder_report = Path(encoder_report)
        self.binary = self.work / "avfoundation_probe"
        self.encoder_cases = []
        self.encoder_work = None
        self.runtime_directory = None
        removed = sorted(key for key in self.environment if key.startswith(("DYLD_", "LD_", "__XPC_DYLD_"))
                         or key in ("ASAN_OPTIONS", "UBSAN_OPTIONS"))
        for key in removed:
            del self.environment[key]
        self.environment["MACOSX_DEPLOYMENT_TARGET"] = "15.0"
        if sanitizers:
            self.environment.update(ASAN_OPTIONS="halt_on_error=1:abort_on_error=1:exitcode=86:detect_leaks=0",
                                    UBSAN_OPTIONS="halt_on_error=1:abort_on_error=1:exitcode=87:print_stacktrace=1")
        self.report.update(scope="AVFoundation asset-range reader timing of retained encoder files, not acoustic playback",
                           experiment_completed=False, removed_environment_override_names=removed,
                           sanitizer_options={key: self.environment[key] for key in ("ASAN_OPTIONS", "UBSAN_OPTIONS")
                                              if key in self.environment},
                           unqualified=["physical playback/listening", "coded padding outside the default asset range",
                                        "video/color/closed GOP observations", "edge-content survival", "product export integration"])

    def admit(self, path, maximum, *, expected=None, expected_bytes=None):
        data, artifact = bounded_file(path, maximum)
        if expected is not None and artifact["sha256"] != expected:
            raise ValueError(f"recorded SHA-256 mismatch: {path}")
        if expected_bytes is not None and artifact["bytes"] != expected_bytes:
            raise ValueError(f"recorded byte count mismatch: {path}")
        self.admitted_files[str(path)] = artifact["sha256"]
        self.loaded_paths[str(path)] = {"resolved": str(Path(path).resolve()), "sha256": artifact["sha256"]}
        return data, artifact

    def load_encoder_report(self):
        data, artifact = self.admit(self.encoder_report, MAX_INPUT)
        self.report["encoder_report"] = artifact
        value = strict_json(data)
        self.assert_that("completed immutable encoder observation report", isinstance(value, dict)
                         and exact(value.get("schema_version"), 1) and value.get("experiment_completed") is True
                         and value.get("source_unchanged_during_run") is True)
        cases = value.get("cases")
        self.assert_that("bounded encoder case inventory", isinstance(cases, list) and 1 <= len(cases) <= 64)
        work = value.get("work_directory")
        self.assert_that("recorded absolute encoder work directory", isinstance(work, str) and Path(work).is_absolute())
        self.encoder_work = Path(work).resolve()
        self.encoder_cases = cases
        self.report["encoder_result"] = value.get("result")
        # A failed timing oracle is the reason for this consumer experiment.
        # It is not a failed encoder invocation or a reason to generate new bytes.

    def admit_linkage(self, binary, output, search_paths, *, runtime=False):
        records = []
        for name in dependencies(output):
            if system_dependency(name):
                records.append({"load_path": name, "kind": "system"})
                continue
            base = Path(name).name
            if not self.sanitizers or base not in RUNTIME_NAMES or self.runtime_directory is None:
                raise ValueError(f"unadmitted non-system dependency: {name}")
            expected = (self.runtime_directory / base).resolve()
            selected_path = Path(name)
            if name.startswith("@rpath/"):
                if name != "@rpath/" + base:
                    raise ValueError(f"unrecognized sanitizer load path: {name}")
                candidates = []
                for search in search_paths:
                    expanded = search.replace("@executable_path", str(self.binary.parent)).replace("@loader_path", str(binary.parent))
                    if not Path(expanded).is_absolute():
                        raise ValueError(f"unresolved runtime search path: {search}")
                    candidate = Path(expanded) / base
                    if candidate.exists():
                        candidates.append(candidate)
                if not candidates or candidates[0].resolve() != expected:
                    raise ValueError(f"sanitizer runtime does not resolve to selected compiler: {name}")
                selected_path = candidates[0]
            elif not Path(name).is_absolute() or Path(name).resolve() != expected:
                raise ValueError(f"sanitizer runtime is outside selected compiler: {name}")
            _, artifact = self.admit(expected, MAX_INPUT)
            observed = {"resolved": str(selected_path.resolve()), "sha256": digest(selected_path)}
            if observed != {"resolved": str(expected), "sha256": artifact["sha256"]}:
                raise ValueError(f"sanitizer candidate changed during admission: {selected_path}")
            self.loaded_paths[str(selected_path)] = observed
            records.append({"load_path": name, "candidate_path": str(selected_path),
                            "kind": "compiler sanitizer runtime", **artifact})
            if not runtime:
                linked = self.run(["otool", "-L", expected]).stdout
                # Runtime self-ID uses the same executable search context.
                children = self.admit_linkage(expected, linked, search_paths, runtime=True)
                records[-1].update(linked_libraries=linked, dependencies=children)
        return records

    def prepare(self):
        self.load_encoder_report()
        self.assert_that("Apple Silicon macOS", platform.system() == "Darwin" and platform.machine() == "arm64")
        sdk = self.run(["xcrun", "--sdk", "macosx", "--show-sdk-path"]).stdout.strip()
        compiler = Path(self.run(["xcrun", "--sdk", "macosx", "--find", "clang"]).stdout.strip()).resolve()
        compiler_artifact = self.artifact(compiler)
        self.admitted_files[str(compiler)] = compiler_artifact["sha256"]
        compiler_version = self.run(["xcrun", "--sdk", "macosx", "clang", "--version"]).stdout
        self.assert_that("selected Apple compiler", "Apple clang version" in compiler_version)
        self.report["host"] = {"platform": platform.platform(), "machine": platform.machine(),
            "processor": self.run(["sysctl", "-n", "machdep.cpu.brand_string"]).stdout.strip(),
            "memory_bytes": int(self.run(["sysctl", "-n", "hw.memsize"]).stdout),
            "macos": self.run(["sw_vers"]).stdout.strip(), "sdk_path": sdk,
            "sdk_version": self.run(["xcrun", "--sdk", "macosx", "--show-sdk-version"]).stdout.strip(),
            "compiler": {**compiler_artifact, "version": compiler_version}}
        flags = ["-std=gnu11", "-O2", "-g", "-Wall", "-Wextra", "-Werror", "-fobjc-arc",
                 "-fobjc-arc-exceptions", "-fblocks", "-arch", "arm64", "-mmacosx-version-min=15.0",
                 "-isysroot", sdk]
        if self.sanitizers:
            resource = Path(self.run(["xcrun", "--sdk", "macosx", "clang", "-print-resource-dir"]).stdout.strip()).resolve()
            self.runtime_directory = resource / "lib/darwin"
            self.report["compiler_runtime_directory"] = str(self.runtime_directory)
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer", f"-Wl,-rpath,{self.runtime_directory}"]
        self.run(["xcrun", "--sdk", "macosx", "clang", *flags, ROOT / "avfoundation_probe.m", "-o", self.binary,
                  "-framework", "Foundation", "-framework", "AVFoundation", "-framework", "CoreMedia",
                  "-framework", "AudioToolbox"])
        _, artifact = self.admit(self.binary, MAX_INPUT)
        linked = self.run(["otool", "-L", self.binary]).stdout
        loads = self.run(["otool", "-l", self.binary]).stdout
        version = self.run(["vtool", "-show-build", self.binary]).stdout
        self.report["binary"] = {**artifact, "linked_libraries": linked, "load_commands": loads, "build_version": version}
        self.assert_that("native probe targets macOS 15", re.search(r"\bminos\s+15\.0\b", version) is not None)
        self.report["binary"]["dependencies"] = self.admit_linkage(self.binary, linked, rpaths(loads))

    def capture_case(self, name):
        case = {"name": name, "status": "failed", "declared_encoded_tolerance_samples": 799}
        self.report["cases"].append(case)
        pcm_path = self.work / f"{name}.native.f32"
        command = None
        try:
            matches = [value for value in self.encoder_cases if isinstance(value, dict) and value.get("name") == name]
            if len(matches) != 1:
                raise ValueError("encoder report must contain exactly one matching case")
            source = require_fixture(matches[0], name)
            case["encoder_source"] = source
            artifacts = matches[0].get("artifacts")
            recorded = artifacts.get("mp4") if isinstance(artifacts, dict) else None
            if not isinstance(recorded, dict) or not isinstance(recorded.get("path"), str):
                raise ValueError("missing retained MP4 artifact")
            path = Path(recorded["path"])
            if not path.is_absolute() or path.name != name + ".mp4" or path.parent.resolve() != self.encoder_work:
                raise ValueError("MP4 path is outside its recorded encoder case")
            sha, size = recorded.get("sha256"), recorded.get("bytes")
            if not isinstance(sha, str) or re.fullmatch(r"[0-9a-f]{64}", sha) is None or type(size) is not int or not 1 <= size <= MAX_INPUT:
                raise ValueError("invalid retained MP4 artifact identity")
            _, case["input"] = self.admit(path, MAX_INPUT, expected=sha, expected_bytes=size)
            command = len(self.report["commands"])
            result = self.run([self.binary, path, pcm_path], required=False)
            case["probe_exit_code"] = result.returncode
            observation = strict_json(result.stdout)
            case["native_observation"] = observation
            if not isinstance(observation, dict) or any(observation.get(key) != sha for key in ("input_sha256", "final_input_sha256")):
                raise ValueError("native reader did not retain the admitted input hash")
            data, case["pcm"] = bounded_file(pcm_path, MAX_PCM, allow_empty=True)
            if len(data) % 8:
                raise ValueError("PCM contains a partial interleaved stereo frame")
            pcm = array.array("f")
            if pcm.itemsize != 4:
                raise ValueError("host float array is not float32")
            pcm.frombytes(data)
            if sys.byteorder != "little":
                pcm.byteswap()
            outcome = inspect_native_audio(CaseSpec(frame_count=120, fps_num=60, fps_den=1), observation, pcm,
                                           tolerance_samples=799)
            case["oracle"] = outcome
            # The complete oracle includes this same raw observation; avoid duplicating it in JSON.
            del case["native_observation"]
            case["status"] = outcome["outcome"] if result.returncode == 0 else "failed"
            if result.returncode != 0:
                case["failure"] = "native probe process failed; retained raw reader observations and oracle outcome"
        except (AssertionError, ValueError, OSError, RuntimeError, KeyError, TypeError) as error:
            case["failure"] = str(error)
        finally:
            if command is not None and command < len(self.report["commands"]):
                case["command"] = command
                case["raw_logs"] = self.report["commands"][command].get("logs", {})
            if pcm_path.is_file() and "pcm" not in case:
                case["pcm"] = self.artifact(pcm_path)
        return case

    def capture_all(self):
        for name in CASE_NAMES:
            self.capture_case(name)
        self.report["experiment_completed"] = True
        statuses = [case["status"] for case in self.report["cases"]]
        self.report["result"] = (PASS_RESULT if statuses == ["passed", "passed"] else
                                 "completed: AVFoundation reader timing remains unqualified" if "failed" not in statuses else
                                 "failed: native reader or timing checks")
        if self.process_faults:
            self.report["result"] = "failed: native process fault"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--encoder-report", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--sanitizers", action="store_true")
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()):
        parser.error("work directory must be empty")
    if args.output.exists():
        parser.error("output report must not already exist")
    harness = NativeAudioHarness(work, args.sanitizers, args.encoder_report.resolve())
    try:
        harness.report["source_sha256_at_start"] = source_inventory()
        harness.prepare()
        harness.capture_all()
    except (AssertionError, ValueError, OSError, RuntimeError, KeyError, TypeError) as error:
        harness.report["failure"] = str(error)
        harness.report["result"] = "failed: native experiment preparation"
    finally:
        harness.finish_admission()
        try:
            harness.report["source_sha256"] = source_inventory()
            unchanged = harness.report["source_sha256"] == harness.report.get("source_sha256_at_start")
        except OSError as error:
            unchanged = False
            harness.report["source_inventory_error"] = str(error)
        harness.report["source_unchanged_during_run"] = unchanged
        if not unchanged:
            harness.report["result"] = "failed: source changed during the run"
        harness.report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("x", encoding="utf-8") as target:
            json.dump(harness.report, target, indent=2, allow_nan=False)
            target.write("\n")
        print(json.dumps({"result": harness.report["result"], "report": str(args.output),
                          "experiment_completed": harness.report["experiment_completed"]}), flush=True)
    return 0 if harness.report["result"] == PASS_RESULT else 1


if __name__ == "__main__":
    raise SystemExit(main())
