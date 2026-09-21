#!/usr/bin/env python3
"""Qualify a lossless RGB8 FFV1 v3 Matroska master with the isolated prefix.

This is a developer-only measurement harness. It never uses a system FFmpeg,
keeps generated media under the requested scratch directory, and reports the
Matroska timebase separately from the source's exact rational clock.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any


ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
MAX_FILE_BYTES = 2 * 1024 * 1024 * 1024
EXPECTED_CANDIDATE_SHA256 = "d7fcf04e2bbe213d0352153443eb77c4a1534855ab8c19c0ff5dde4f2ddf85d9"
EXPECTED_NATIVE_SHA256 = "c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33"
INPUTS = (
    {
        "name": "candidate-snapshot",
        "path": Path("/tmp/deadpan-supervised-mlx-20260921-attributed/host/candidate.snapshot.mp4"),
        "sha256": EXPECTED_CANDIDATE_SHA256,
        "width": 768,
        "height": 320,
        "frames": 30,
        "time_base": [1, 30000],
        "frame_rate": [30000, 1001],
        "pts": [index * 1001 for index in range(30)],
        "durations": [1001] * 30,
    },
    {
        "name": "native-25-sequence",
        "path": Path("/tmp/deadpan-supervised-mlx-20260921-attributed/worker/outputs/native.mp4"),
        "sha256": EXPECTED_NATIVE_SHA256,
        "width": 768,
        "height": 320,
        "frames": 25,
        "time_base": [1, 24],
        "frame_rate": [24, 1],
        "pts": list(range(25)),
        "durations": [1] * 25,
    },
)


def sha256_file(path: Path) -> tuple[str, int]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            size += len(chunk)
            if size > MAX_FILE_BYTES:
                raise ValueError(f"file exceeds qualification bound: {path}")
            digest.update(chunk)
    return digest.hexdigest(), size


def parse_json_line(text: str) -> dict[str, Any]:
    """Parse the probe's one-line JSON contract without accepting extra output."""
    lines = [line for line in text.splitlines() if line.strip()]
    if len(lines) != 1:
        raise ValueError(f"probe stdout must contain exactly one JSON line, got {len(lines)}")
    def reject_constant(value: str) -> None:
        raise ValueError(f"invalid JSON constant: {value}")

    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON field: {key}")
            result[key] = value
        return result

    value = json.loads(
        lines[0], parse_constant=reject_constant, object_pairs_hook=reject_duplicates
    )
    if not isinstance(value, dict):
        raise ValueError("probe result must be a JSON object")
    return value


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def integer(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def integer_sequence(value: Any, length: int) -> bool:
    return isinstance(value, list) and len(value) == length and all(integer(item) for item in value)


def nearest_millisecond(pts: int, time_base: list[int]) -> int:
    numerator = pts * time_base[0] * 1000
    denominator = time_base[1]
    return (2 * numerator + denominator) // (2 * denominator)


def validate_probe_result(value: dict[str, Any], expected: dict[str, Any]) -> None:
    require(value.get("status") == "passed", "native probe did not pass")
    source = value.get("source")
    output = value.get("output")
    require(isinstance(source, dict), "probe source metadata is missing")
    require(isinstance(output, dict), "probe output metadata is missing")
    require(
        all(integer(source.get(field)) for field in ("width", "height", "frames", "start_pts")),
        "source integer metadata is malformed",
    )
    require(integer_sequence(source.get("time_base"), 2), "source time base is malformed")
    require(integer_sequence(source.get("frame_rate"), 2), "source frame rate is malformed")
    require(integer_sequence(source.get("native_pts"), expected["frames"]), "source PTS sequence is malformed")
    require(integer_sequence(source.get("durations"), expected["frames"]), "source duration sequence is malformed")
    for field in ("width", "height", "frames", "time_base", "frame_rate"):
        require(source.get(field) == expected[field], f"source {field} differs from manifest")
    require(source.get("start_pts") == expected["pts"][0], "source start PTS differs from manifest")
    require(source.get("native_pts") == expected["pts"], "source PTS sequence differs from manifest")
    require(source.get("durations") == expected["durations"], "source durations differ from manifest")
    require(output.get("codec") == "ffv1", "output codec is not FFV1")
    require(output.get("pixel_format") == "bgr0", "output pixel format is not BGR0")
    require(output.get("ffv1_version") == 3, "output is not FFV1 version 3")
    require(output.get("slice_crc") is True, "output does not report slice CRC")
    require(output.get("color_range") == "full", "output range tag changed")
    require(output.get("color_space") == "gbr", "output matrix tag changed")
    require(output.get("color_transfer") == "sRGB", "output transfer tag changed")
    require(output.get("color_primaries") == "bt709", "output primaries tag changed")
    require(output.get("frames") == expected["frames"], "output frame count changed")
    require(integer(output.get("frames")), "output frame count is malformed")
    require(integer_sequence(output.get("time_base"), 2), "output time base is malformed")
    require(integer_sequence(output.get("frame_rate"), 2), "output frame rate is malformed")
    require(output.get("time_base") == [1, 1000], "Matroska time base is not 1/1000")
    require(output.get("frame_rate") == expected["frame_rate"], "output frame rate changed")
    output_pts = output.get("native_pts")
    output_durations = output.get("durations")
    require(
        isinstance(output_pts, list)
        and len(output_pts) == expected["frames"]
        and all(integer(item) for item in output_pts),
        "output PTS sequence is malformed",
    )
    require(
        isinstance(output_durations, list)
        and len(output_durations) == expected["frames"]
        and all(integer(item) and item > 0 for item in output_durations),
        "output duration sequence is malformed",
    )
    require(
        all(left < right for left, right in zip(output_pts, output_pts[1:])),
        "output PTS sequence is not strictly increasing",
    )
    expected_output_pts = [nearest_millisecond(pts, expected["time_base"]) for pts in expected["pts"]]
    require(output_pts == expected_output_pts, "output PTS are not nearest-millisecond rescaling")
    default_duration = 1000 * expected["frame_rate"][1] // expected["frame_rate"][0]
    require(default_duration > 0, "expected frame rate has no positive millisecond duration")
    require(
        output_durations == [default_duration] * expected["frames"],
        "output DefaultDuration does not match the frame rate floor",
    )


def selected_probe(value: dict[str, Any]) -> dict[str, Any]:
    streams = value.get("streams")
    require(isinstance(streams, list) and len(streams) == 1, "FFV1 output must have one stream")
    stream = streams[0]
    require(isinstance(stream, dict), "ffprobe stream must be an object")
    require(stream.get("codec_type") == "video", "FFV1 output stream must be video")
    selected = {
        key: stream.get(key)
        for key in (
            "codec_name",
            "codec_type",
            "width",
            "height",
            "pix_fmt",
            "color_range",
            "color_space",
            "color_transfer",
            "color_primaries",
            "time_base",
            "r_frame_rate",
            "avg_frame_rate",
            "nb_frames",
            "tags",
        )
    }
    require(stream.get("codec_name") == "ffv1", "output codec is not FFV1")
    require(stream.get("pix_fmt") == "bgr0", "output is not FFV1's lossless 8-bit BGR0")
    require(stream.get("color_range") == "pc", "output is not full range")
    require(stream.get("color_space") == "gbr", "output color space is not GBR")
    require(stream.get("color_transfer") == "iec61966-2-1", "output transfer is not sRGB")
    require(stream.get("color_primaries") == "bt709", "output primaries are not BT.709")
    fmt = value.get("format")
    require(isinstance(fmt, dict) and str(fmt.get("format_name", "")).startswith("matroska"),
            "output container is not Matroska")
    return selected


class Harness:
    def __init__(self, prefix: Path, work: Path, sanitizers: bool):
        self.prefix = prefix.resolve()
        self.work = work.resolve()
        self.sanitizers = sanitizers
        self.binary = self.work / ("ffv1_probe_asan" if sanitizers else "ffv1_probe")
        self.commands: list[dict[str, Any]] = []
        self.report: dict[str, Any] = {
            "schema_version": 1,
            "scope": "developer canonical lossless FFV1 master qualification; not app integration, Gate A, or release acceptance",
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "sanitizers": sanitizers,
            "work_directory": str(self.work),
            "prefix": str(self.prefix),
            "commands": self.commands,
            "cases": [],
            "negative_controls": [],
            "decoder_limitations": [],
            "source_sha256": {
                "tools/media-qualification/ffv1/ffv1_probe.c": sha256_file(ROOT / "ffv1_probe.c")[0],
                "tools/media-qualification/ffv1/qualify.py": sha256_file(ROOT / "qualify.py")[0],
                "tools/media-qualification/ffv1/test_qualify.py": sha256_file(ROOT / "test_qualify.py")[0],
            },
        }

    def run(self, argv: list[Path | str], *, expected: int = 0, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        started = time.monotonic()
        command = [str(item) for item in argv]
        try:
            result = subprocess.run(
                command,
                cwd=self.work,
                env=env,
                check=False,
                capture_output=True,
                text=True,
                timeout=180,
            )
        except subprocess.TimeoutExpired as error:
            self.commands.append({
                "argv": command,
                "cwd": str(self.work),
                "timeout_seconds": 180,
                "elapsed_seconds": round(time.monotonic() - started, 6),
                "status": "timed_out",
            })
            raise RuntimeError(f"command timed out after 180 seconds: {' '.join(command)}") from error
        self.commands.append({
            "argv": command,
            "cwd": str(self.work),
            "returncode": result.returncode,
            "expected_returncode": expected,
            "elapsed_seconds": round(time.monotonic() - started, 6),
            "stdout": result.stdout if len(result.stdout) <= 8192 else result.stdout[:2048],
            "stdout_truncated": len(result.stdout) > 8192,
            "stdout_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
            "stderr": result.stderr if len(result.stderr) <= 8192 else result.stderr[:2048],
            "stderr_truncated": len(result.stderr) > 8192,
            "stderr_sha256": hashlib.sha256(result.stderr.encode()).hexdigest(),
        })
        if result.returncode != expected:
            raise RuntimeError(f"command returned {result.returncode}, expected {expected}: {' '.join(command)}\n{result.stderr}")
        return result

    def environment(self) -> dict[str, str]:
        env = dict(os.environ)
        env["DYLD_LIBRARY_PATH"] = str(self.prefix / "lib")
        env["PATH"] = f"{self.prefix / 'bin'}{os.pathsep}{env.get('PATH', '')}"
        if self.sanitizers:
            env["ASAN_OPTIONS"] = "halt_on_error=1:abort_on_error=1:exitcode=86:detect_leaks=0"
            env["UBSAN_OPTIONS"] = "halt_on_error=1:abort_on_error=1:exitcode=87:print_stacktrace=1"
        return env

    def probe_command(self) -> str:
        return f"./{self.binary.name}"

    def build(self) -> None:
        include = self.prefix / "include"
        lib = self.prefix / "lib"
        require(include.is_dir() and lib.is_dir(), "isolated FFmpeg prefix is incomplete")
        flags = [
            "clang", "-std=c11", "-O2", "-g", "-Wall", "-Wextra", "-Werror",
            f"-I{include}", str(ROOT / "ffv1_probe.c"),
            f"-L{lib}", f"-Wl,-rpath,{lib}", "-lavformat", "-lavcodec", "-lavutil", "-lswscale",
            "-o", str(self.binary),
        ]
        if self.sanitizers:
            flags[2:2] = ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
        self.run(flags)
        self.report["binary"] = {"path": str(self.binary), "sha256": sha256_file(self.binary)[0], "size_bytes": self.binary.stat().st_size}
        otool = self.run(["otool", "-L", self.binary]).stdout
        linked_ffmpeg = []
        for line in otool.splitlines()[1:]:
            linked = line.strip().split(" ", 1)[0]
            if linked.startswith(("/tmp/", "/private/tmp/")) and (
                "libav" in linked or "libswscale" in linked
            ):
                linked_ffmpeg.append(Path(linked).resolve())
        expected_library = (self.prefix / "lib").resolve()
        require(
            len(linked_ffmpeg) == 4
            and all(path.parent == expected_library for path in linked_ffmpeg),
            "probe does not link exactly four selected isolated FFmpeg libraries",
        )
        require("Cellar" not in otool and "homebrew" not in otool.lower(), "probe linked a Homebrew library")
        self.report["linked_libraries"] = otool
        ffprobe = self.prefix / "bin" / "ffprobe"
        require(ffprobe.is_file(), "isolated ffprobe is missing")
        self.report["ffprobe_version"] = self.run(["ffprobe", "-version"], env=self.environment()).stdout.splitlines()[0]
        self.report["library_sha256"] = {
            path.name: {"path": str(path), "sha256": sha256_file(path)[0]}
            for path in sorted(set(linked_ffmpeg))
        }
        receipt_path = REPO / "tools/media-qualification/compatible/results/build-2026-09-20.json"
        receipt_hash, receipt_size = sha256_file(receipt_path)
        receipt = json.loads(receipt_path.read_text())
        configure_argv = receipt.get("configure_argv")
        license_hashes = receipt.get("license_file_sha256")
        require(isinstance(configure_argv, list) and configure_argv, "build receipt has no configure argv")
        require(isinstance(license_hashes, dict) and license_hashes, "build receipt has no license hashes")
        self.report["build_receipt"] = {
            "path": str(receipt_path),
            "sha256": receipt_hash,
            "size_bytes": receipt_size,
            "ffmpeg": receipt.get("pins", {}).get("ffmpeg"),
            "configure_argv": configure_argv,
            "license_file_sha256": license_hashes,
        }
        model_path = REPO / "docs/qualification/model-worker-2026-09-21.md"
        model_hash, model_size = sha256_file(model_path)
        self.report["input_provenance"] = {
            "path": str(model_path),
            "sha256": model_hash,
            "size_bytes": model_size,
            "captured_run": "/tmp/deadpan-supervised-mlx-20260921-attributed",
        }

    def ffprobe(self, path: Path) -> dict[str, Any]:
        result = self.run([
            "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", path,
        ], env=self.environment())
        return json.loads(result.stdout)

    def negative_controls(self, master: Path, source: Path, name: str) -> None:
        tail_truncated = self.work / f"{name}.tail-truncated.mkv"
        shutil.copyfile(master, tail_truncated)
        with tail_truncated.open("r+b") as stream:
            stream.seek(-1, 2)
            stream.truncate()
        result = self.run(
            [self.probe_command(), "verify", tail_truncated, source],
            expected=0,
            env=self.environment(),
        )
        tail_result = parse_json_line(result.stdout)
        require(tail_result.get("status") == "passed", "tail-truncated decode did not report pass")
        self.report["decoder_limitations"].append({
            "case": name,
            "kind": "last_byte_truncation_not_detected_by_pixel_decode",
            "input_sha256": sha256_file(tail_truncated)[0],
            "returncode": result.returncode,
            "stderr": result.stderr,
            "host_requirement": "reject changed byte length or SHA-256 before decode",
        })
        truncated = self.work / f"{name}.truncated.mkv"
        shutil.copyfile(master, truncated)
        with truncated.open("r+b") as stream:
            stream.truncate(max(128, master.stat().st_size // 2))
        result = self.run([self.probe_command(), "verify", truncated, source], expected=1, env=self.environment())
        self.report["negative_controls"].append({
            "case": name,
            "kind": "truncated_matroska",
            "input_sha256": sha256_file(truncated)[0],
            "returncode": result.returncode,
            "stderr": result.stderr,
        })
        corrupted = self.work / f"{name}.corrupted.mkv"
        shutil.copyfile(master, corrupted)
        with corrupted.open("r+b") as stream:
            size = corrupted.stat().st_size
            offset = max(128, size // 2)
            count = min(4096, size - offset - 16)
            require(count > 0, "master is too short for corruption control")
            stream.seek(offset)
            stream.write(b"\0" * count)
        result = self.run([self.probe_command(), "verify", corrupted, source], expected=1, env=self.environment())
        self.report["negative_controls"].append({
            "case": name,
            "kind": "corrupted_matroska",
            "input_sha256": sha256_file(corrupted)[0],
            "returncode": result.returncode,
            "stderr": result.stderr,
        })
        mutated = self.work / f"{name}.mutated.rgb"
        shutil.copyfile(source, mutated)
        with mutated.open("r+b") as stream:
            stream.seek(64 + 16 + 7)
            original = stream.read(1)
            require(len(original) == 1, "source fixture is too short for mutation")
            stream.seek(-1, 1)
            stream.write(bytes([original[0] ^ 0x01]))
        result = self.run([self.probe_command(), "verify", master, mutated], expected=1, env=self.environment())
        self.report["negative_controls"].append({
            "case": name,
            "kind": "mutated_source_pixel",
            "input_sha256": sha256_file(mutated)[0],
            "returncode": result.returncode,
            "stderr": result.stderr,
        })

        raw_controls = (
            ("zero_header", 0, b"\0" * 64),
            ("invalid_dimensions", 12, (4097).to_bytes(4, "little")),
            ("zero_frames", 20, b"\0" * 4),
        )
        for kind, offset, replacement in raw_controls:
            malformed = self.work / f"{name}.{kind}.rgb"
            shutil.copyfile(source, malformed)
            with malformed.open("r+b") as stream:
                stream.seek(offset)
                stream.write(replacement)
            result = self.run([self.probe_command(), "verify", master, malformed], expected=1, env=self.environment())
            self.report["negative_controls"].append({
                "case": name,
                "kind": f"raw_{kind}",
                "input_sha256": sha256_file(malformed)[0],
                "returncode": result.returncode,
                "stderr": result.stderr,
            })
        wrong_size = self.work / f"{name}.wrong-size.rgb"
        shutil.copyfile(source, wrong_size)
        with wrong_size.open("ab") as stream:
            stream.write(b"\0")
        result = self.run([self.probe_command(), "verify", master, wrong_size], expected=1, env=self.environment())
        self.report["negative_controls"].append({
            "case": name,
            "kind": "raw_size_mismatch",
            "input_sha256": sha256_file(wrong_size)[0],
            "returncode": result.returncode,
            "stderr": result.stderr,
        })

    def qualify_case(self, expected: dict[str, Any]) -> None:
        name = expected["name"]
        input_path = expected["path"]
        input_hash, input_size = sha256_file(input_path)
        require(input_hash == expected["sha256"], f"{name}: supplied input hash changed")
        master = self.work / f"{name}.mkv"
        source = self.work / f"{name}.rgb"
        result = self.run([self.probe_command(), "qualify", input_path, master, source], env=self.environment())
        native = parse_json_line(result.stdout)
        validate_probe_result(native, expected)
        probe = selected_probe(self.ffprobe(master))
        require(probe["width"] == expected["width"] and probe["height"] == expected["height"],
                f"{name}: ffprobe dimensions changed")
        master_hash, master_size = sha256_file(master)
        source_hash, source_size = sha256_file(source)
        self.report["cases"].append({
            "name": name,
            "input": {"path": str(input_path), "sha256": input_hash, "size_bytes": input_size},
            "decoded_rgb_source": {"path": str(source), "sha256": source_hash, "size_bytes": source_size},
            "master": {"path": str(master), "sha256": master_hash, "size_bytes": master_size},
            "native_result": native,
            "ffprobe": probe,
            "status": "passed exact RGB pixel and ordinal comparison",
        })
        self.negative_controls(master, source, name)

    def qualify_synthetic(self) -> None:
        master = self.work / "synthetic-32x16.mkv"
        source = self.work / "synthetic-32x16.rgb"
        result = self.run([self.probe_command(), "synth", master, source], env=self.environment())
        native = parse_json_line(result.stdout)
        validate_probe_result(native, {
            "width": 32,
            "height": 16,
            "frames": 30,
            "time_base": [1, 30000],
            "frame_rate": [30000, 1001],
            "pts": [index * 1001 for index in range(30)],
            "durations": [1001] * 30,
        })
        probe = selected_probe(self.ffprobe(master))
        self.report["cases"].append({
            "name": "synthetic-32x16-30-cfr",
            "input": {"kind": "deterministic RGB8 pattern", "sha256": None},
            "decoded_rgb_source": {"path": str(source), "sha256": sha256_file(source)[0], "size_bytes": source.stat().st_size},
            "master": {"path": str(master), "sha256": sha256_file(master)[0], "size_bytes": master.stat().st_size},
            "native_result": native,
            "ffprobe": probe,
            "status": "passed exact RGB pixel and ordinal comparison",
        })
        self.negative_controls(master, source, "synthetic-32x16")

    def execute(self, inputs: tuple[dict[str, Any], ...]) -> None:
        self.build()
        self.qualify_synthetic()
        for expected in inputs:
            self.qualify_case(expected)
        self.report["status"] = "passed scoped qualification"
        self.report["completed_utc"] = datetime.now(timezone.utc).isoformat()


def default_inputs() -> tuple[dict[str, Any], ...]:
    missing = [str(item["path"]) for item in INPUTS if not item["path"].is_file()]
    if missing:
        raise RuntimeError(f"required supplied RGB inputs are missing: {', '.join(missing)}")
    return INPUTS


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path("/tmp/deadpan-media-compatible-xyhilms4/prefix"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--sanitizers", action="store_true")
    args = parser.parse_args(argv)
    work = args.work_dir.resolve() if args.work_dir else Path(tempfile.mkdtemp(prefix="deadpan-ffv1-", dir="/tmp"))
    work.mkdir(parents=True, exist_ok=False) if not work.exists() else None
    harness = Harness(args.prefix, work, args.sanitizers)
    try:
        inputs = default_inputs()
        harness.execute(inputs)
    except (AssertionError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        harness.report["status"] = "failed"
        harness.report["failure"] = str(error)
        harness.report["completed_utc"] = datetime.now(timezone.utc).isoformat()
        print(str(error), file=sys.stderr)
        exit_code = 1
    else:
        exit_code = 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(harness.report, indent=2) + "\n")
    print(f"Report: {args.output}\nFixtures: {work}\nResult: {harness.report['status']}")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
