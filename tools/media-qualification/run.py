#!/usr/bin/env python3
"""Build and run the isolated native media harness; Python is developer-only.

The report records failures and exits nonzero on any required assertion failure.
An unavailable VideoToolbox OS software encoder is a recorded capability result,
not permission to silently fall back to a GPL encoder in the application.
"""

from __future__ import annotations

import argparse
import array
from datetime import datetime, timezone
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import platform
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parent


def digest(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


class Harness:
    def __init__(self, work: Path, sanitizers: bool):
        self.work = work
        self.sanitizers = sanitizers
        self.report: dict = {
            "schema_version": 1,
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "scope": "developer native media qualification, not Gate A or release acceptance",
            "sanitizers": sanitizers,
            "work_directory": str(work),
            "commands": [],
            "cases": [],
            "assertions": [],
            "source_sha256": {name: digest(ROOT / name) for name in ("run.py", "media_probe.c")},
        }

    def run(self, argv: list[str], *, required: bool = True, timeout: int = 120) -> subprocess.CompletedProcess:
        started = time.monotonic()
        try:
            result = subprocess.run(
                [str(arg) for arg in argv],
                cwd=self.work,
                check=False,
                capture_output=True,
                text=True,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired as error:
            self.report["commands"].append({"argv": [str(arg) for arg in argv], "timeout_seconds": timeout})
            raise RuntimeError(f"command timed out: {argv[0]}") from error
        self.report["commands"].append({
            "argv": [str(arg) for arg in argv],
            "exit_code": result.returncode,
            "elapsed_seconds": round(time.monotonic() - started, 6),
            "stdout": result.stdout if len(result.stdout) <= 8192 else result.stdout[:2048],
            "stdout_truncated": len(result.stdout) > 8192,
            "stdout_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
            "stderr": result.stderr,
        })
        if required and result.returncode != 0:
            raise RuntimeError(f"command failed ({result.returncode}): {shlex.join(map(str, argv))}\n{result.stderr}")
        return result

    def assert_that(self, label: str, condition: bool, details=None) -> None:
        self.report["assertions"].append({"label": label, "passed": bool(condition), "details": details})
        if not condition:
            raise AssertionError(f"{label}: {details}")

    def inspect(self, name: str, source: dict) -> dict:
        path = self.work / f"{name}.mp4"
        binary = self.work / "media_probe"
        video = json.loads(self.run([binary, "video", path]).stdout)
        pcm = self.work / f"{name}.f32"
        audio = json.loads(self.run([binary, "audio", path, pcm]).stdout)
        probe = json.loads(self.run([
            "ffprobe", "-v", "error", "-show_streams", "-show_format", "-show_packets",
            "-print_format", "json", path,
        ]).stdout)
        case = {
            "name": name, "fixture": source, "video": video, "audio": audio,
            "sha256": digest(path), "size_bytes": path.stat().st_size,
            "ffprobe_streams": probe["streams"], "ffprobe_format": probe["format"],
        }
        # Store the case before assertions so failed behavior remains inspectable.
        self.report["cases"].append(case)
        frames = video["frames"]
        self.assert_that(f"{name}: all source frames decoded", len(frames) == source["frame_count"])
        self.assert_that(f"{name}: decoded visible numbers match every authored frame index",
                         [frame["authored_identity"] for frame in frames] == list(range(source["frame_count"])))
        tb = Fraction(*video["time_base"])
        source_tb = Fraction(*source["time_base"])
        ticks = source["start_pts"]
        expected_pts = []
        expected_durations = []
        for number in range(source["frame_count"]):
            duration = 1001 * (1 + number % 3 if source["cadence"] == "vfr" else 1)
            expected_pts.append(Fraction(ticks) * source_tb)
            expected_durations.append(Fraction(duration) * source_tb)
            ticks += duration
        actual_pts = [frame["pts"] * tb for frame in frames]
        self.assert_that(f"{name}: exact rational presentation timestamps", actual_pts == expected_pts,
                         {"expected_first": str(expected_pts[0]), "actual_first": str(actual_pts[0]),
                          "expected_last": str(expected_pts[-1]), "actual_last": str(actual_pts[-1])})
        # With reordered VFR packets, raw decoded duration can describe a decode
        # interval, not the distance to the next presentation timestamp. Preserve
        # that observation and qualify the presentation index explicitly.
        raw_durations = [frame["duration"] * tb for frame in frames]
        presentation_durations = [right - left for left, right in zip(actual_pts, actual_pts[1:])]
        presentation_durations.append(raw_durations[-1])
        case["raw_duration_mismatch_count"] = sum(a != b for a, b in zip(raw_durations, expected_durations))
        case["presentation_durations"] = [str(value) for value in presentation_durations]
        case["stream_duration_error_ticks"] = video["stream_duration"] - source["duration_ticks"]
        self.assert_that(f"{name}: exact indexed presentation durations", presentation_durations == expected_durations,
                         {"expected_last": str(expected_durations[-1]), "actual_last": str(presentation_durations[-1])})
        self.assert_that(f"{name}: distinguishable frame identities", len({frame["md5"] for frame in frames}) == len(frames))
        self.assert_that(f"{name}: every frame plus repeated seeks hash-match linear decode",
                         len(video["seeks"]) == 12 + len(frames)
                         and {seek["frame"] for seek in video["seeks"]} == set(range(len(frames))))
        self.assert_that(f"{name}: refcounted frame survives decoder reuse", video["retained_frame_survived_flush"])
        expected_patches = [[16, 128, 128], [235, 128, 128], [63, 102, 240], [173, 42, 26], [32, 240, 118]]
        patch_error = max(abs(actual - expected) for patch, reference in
                          zip(video["first_frame_yuv_patches"], expected_patches)
                          for actual, expected in zip(patch, reference))
        self.assert_that(f"{name}: decoded YUV patch centers within 4 code values", patch_error <= 4,
                         {"maximum_error": patch_error, "actual": video["first_frame_yuv_patches"]})
        self.assert_that(f"{name}: actual B-frame presence matches requested mode",
                         (video["b_frames"] > 0) == (source["requested_b_frames"] > 0), video["b_frames"])
        self.assert_that(f"{name}: requested GOP interval produces keyframes", video["keyframes"] >= 8, video["keyframes"])
        keyframes = [index for index, frame in enumerate(frames) if frame["keyframe"]]
        maximum_key_interval = max(right - left for left, right in
                                   zip(keyframes, keyframes[1:] + [len(frames)]))
        case["maximum_keyframe_interval_frames"] = maximum_key_interval
        # The product asks for a GOP around half the frame rate, not an exact
        # encoder-specific interpretation of the setter. Keep the observed
        # interval, including VideoToolbox's 16 versus requested 15, visible.
        self.assert_that(f"{name}: actual keyframe interval within one frame of half-rate target",
                         keyframes[0] == 0 and 14 <= maximum_key_interval <= 16,
                         {"requested": 15, "actual_maximum": maximum_key_interval})
        consecutive = maximum_b_run = 0
        for frame in frames:
            consecutive = consecutive + 1 if frame["type"] == "B" else 0
            maximum_b_run = max(maximum_b_run, consecutive)
        case["maximum_consecutive_b_frames"] = maximum_b_run
        self.assert_that(f"{name}: actual B-frame run does not exceed requested maximum",
                         maximum_b_run <= source["requested_b_frames"], maximum_b_run)
        packet_video = [packet for packet in probe["packets"] if packet["codec_type"] == "video"]
        self.assert_that(f"{name}: actual B-frame timestamp reordering",
                         any(packet["pts"] != packet["dts"] for packet in packet_video)
                         == (source["requested_b_frames"] > 0))
        audio_stream = next(stream for stream in probe["streams"] if stream["codec_type"] == "audio")
        video_stream = next(stream for stream in probe["streams"] if stream["codec_type"] == "video")
        self.assert_that(f"{name}: H.264 and AAC-LC",
                         video_stream["codec_name"] == "h264" and audio_stream["codec_name"] == "aac"
                         and audio_stream["profile"] == "LC")
        self.assert_that(f"{name}: limited-range BT.709 metadata",
                         [video_stream.get(key) for key in ("color_range", "color_space", "color_transfer", "color_primaries")]
                         == ["tv", "bt709", "bt709", "bt709"])
        stream_audio_start = Fraction(audio["stream_start_pts"]) * Fraction(*audio["time_base"]) * 48000
        stream_audio_end = (audio["stream_start_pts"] + audio["stream_duration"]) * Fraction(*audio["time_base"]) * 48000
        case["audio_leading_priming_samples"] = int(source["audio_offset_samples"] - stream_audio_start)
        self.assert_that(f"{name}: audio ends at exact authored sample boundary",
                         stream_audio_end == source["audio_offset_samples"] + source["audio_samples"],
                         {"actual_end": str(stream_audio_end), "expected_end": source["audio_offset_samples"] + source["audio_samples"]})
        self.assert_that(f"{name}: audio leading samples bounded by actual encoder priming",
                         0 <= case["audio_leading_priming_samples"] <= source["audio_encoder_initial_padding"],
                         case["audio_leading_priming_samples"])
        samples = array.array("f")
        with pcm.open("rb") as file:
            samples.frombytes(file.read())
        self.assert_that(f"{name}: decoded PCM matches declared count", len(samples) == 2 * audio["decoded_samples"])
        events = []
        for channel, channel_name in enumerate(("left", "right")):
            for expected_relative in source["impulses"]:
                expected_absolute = source["audio_offset_samples"] + expected_relative
                expected_index = expected_absolute - audio["first_sample_pts"]
                lo, hi = max(0, expected_index - 64), min(audio["decoded_samples"], expected_index + 65)
                self.assert_that(f"{name}: {channel_name} impulse search contains samples", lo < hi)
                peak_index = max(range(lo, hi), key=lambda n: abs(samples[n * 2 + channel]))
                actual = audio["first_sample_pts"] + peak_index
                events.append({"channel": channel_name, "expected_sample": expected_absolute,
                               "actual_peak_sample": actual, "peak_amplitude": samples[peak_index * 2 + channel],
                               "error_samples": actual - expected_absolute})
                self.assert_that(f"{name}: {channel_name} impulse at sample {expected_absolute}",
                                 actual == expected_absolute and abs(samples[peak_index * 2 + channel]) > 0.15, events[-1])
        case["audio_events"] = events
        case["audio_decoded_padding_samples"] = (
            audio["first_sample_pts"] + audio["decoded_samples"]
            - source["audio_offset_samples"] - source["audio_samples"]
        )
        self.assert_that(f"{name}: AAC terminal padding less than one codec frame",
                         0 <= case["audio_decoded_padding_samples"] < 1024,
                         case["audio_decoded_padding_samples"])
        # Actual mux topology, not merely the requested faststart flag.
        atoms = []
        with path.open("rb") as file:
            position = 0
            while position < path.stat().st_size:
                header = file.read(8)
                self.assert_that(f"{name}: complete MP4 atom header", len(header) == 8)
                size = int.from_bytes(header[:4], "big")
                atom = header[4:].decode("ascii")
                if size == 1:
                    size = int.from_bytes(file.read(8), "big")
                if size == 0:
                    size = path.stat().st_size - position
                self.assert_that(f"{name}: valid MP4 atom size", size >= 8 and position + size <= path.stat().st_size)
                atoms.append({"type": atom, "offset": position, "size": size})
                position += size
                file.seek(position)
        case["mp4_atoms"] = atoms
        self.assert_that(f"{name}: moov precedes mdat", next(a["offset"] for a in atoms if a["type"] == "moov")
                         < next(a["offset"] for a in atoms if a["type"] == "mdat"))
        times = [seek["ms"] for seek in video["seeks"]]
        case["seek_summary_ms"] = {"median": statistics.median(times), "max": max(times)}
        return case

    def execute(self) -> None:
        for executable in ("clang", "pkg-config", "ffprobe", "ffmpeg"):
            self.assert_that(f"developer prerequisite: {executable}", shutil.which(executable) is not None)
        self.report["host"] = {"platform": platform.platform(), "machine": platform.machine(),
                               "processor": self.run(["sysctl", "-n", "machdep.cpu.brand_string"]).stdout.strip(),
                               "memory_bytes": int(self.run(["sysctl", "-n", "hw.memsize"]).stdout),
                               "macos": self.run(["sw_vers"]).stdout.strip()}
        self.report["compiler"] = self.run(["clang", "--version"]).stdout
        self.report["ffmpeg_cli"] = self.run(["ffmpeg", "-version"]).stdout
        self.report["pkg_config_versions"] = self.run([
            "pkg-config", "--modversion", "libavformat", "libavcodec", "libavutil"
        ]).stdout.splitlines()
        flags = shlex.split(self.run([
            "pkg-config", "--cflags", "--libs", "libavformat", "libavcodec", "libavutil"
        ]).stdout)
        library_dir = Path(self.run(["pkg-config", "--variable=libdir", "libavcodec"]).stdout.strip())
        self.report["library_files"] = {
            name: {"path": str((library_dir / f"{name}.dylib").resolve()),
                   "sha256": digest(library_dir / f"{name}.dylib")}
            for name in ("libavcodec", "libavformat", "libavutil")
        }
        binary = self.work / "media_probe"
        if self.sanitizers:
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
        self.run(["clang", "-std=c11", "-O2", "-g", "-Wall", "-Wextra", "-Werror",
                  ROOT / "media_probe.c", "-o", binary, *flags])
        self.report["native_libraries"] = json.loads(self.run([binary, "inventory"]).stdout)
        self.report["linked_libraries"] = self.run(["otool", "-L", binary]).stdout
        self.report["binary_sha256"] = digest(binary)
        definitions = [
            ("x264-cfr", "libx264", "cfr", "hardware"),
            ("x264-vfr", "libx264", "vfr", "hardware"),
            ("x264-offset", "libx264", "offset", "hardware"),
            ("videotoolbox-hardware", "h264_videotoolbox", "cfr", "hardware"),
            ("videotoolbox-software", "h264_videotoolbox", "cfr", "software"),
            ("videotoolbox-hardware-no-b", "h264_videotoolbox", "cfr", "hardware-no-b"),
            ("videotoolbox-software-no-b", "h264_videotoolbox", "cfr", "software-no-b"),
        ]
        failures = []
        for name, encoder, cadence, mode in definitions:
            print(f"Qualifying {name}...", flush=True)
            try:
                result = self.run([binary, "encode", self.work / f"{name}.mp4", encoder, cadence, mode])
                case = self.inspect(name, json.loads(result.stdout))
                case["status"] = "passed scoped assertions"
            except (AssertionError, RuntimeError) as error:
                case = next((c for c in self.report["cases"] if c["name"] == name), None)
                if case is None:
                    case = {"name": name}
                    self.report["cases"].append(case)
                case.update({"status": "failed", "failure": str(error)})
                failures.append(name)
        swapped = self.work / "swapped-content.mp4"
        self.run([binary, "encode-swap", swapped, "libx264", "cfr", "hardware"])
        swap_frames = json.loads(self.run([
            "ffprobe", "-v", "error", "-select_streams", "v:0", "-show_frames",
            "-show_entries", "frame=pts", "-print_format", "json", swapped,
        ]).stdout)["frames"]
        self.assert_that("swapped-content negative control preserves all 120 original PTS",
                         [frame["pts"] for frame in swap_frames] == [number * 1001 for number in range(120)])
        rejection = self.run([binary, "video", swapped], required=False)
        self.report["identity_negative_control"] = {
            "fixture_sha256": digest(swapped), "mutation": "swap authored pixels of frames 10 and 11; preserve all PTS",
            "verified_original_pts": [frame["pts"] for frame in swap_frames],
            "probe_exit_code": rejection.returncode, "stderr": rejection.stderr,
        }
        self.assert_that("swapped authored pixels are rejected despite valid frame count and timestamps",
                         rejection.returncode != 0 and "frame identity mismatch: decoded=11 expected=10 pts=10010" in rejection.stderr
                         and "decoded frame identity matches authored index" in rejection.stderr)
        if failures:
            raise AssertionError(f"Media configurations failed qualification: {', '.join(failures)}")
        self.report["status"] = "passed scoped assertions"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="JSON report destination")
    parser.add_argument("--work-dir", type=Path, help="new scratch directory, default /tmp/deadpan-media-*")
    parser.add_argument("--sanitizers", action="store_true", help="enable AddressSanitizer and UndefinedBehaviorSanitizer")
    args = parser.parse_args()
    if args.work_dir:
        work = args.work_dir.resolve()
        work.mkdir(parents=True, exist_ok=False)
    else:
        work = Path(tempfile.mkdtemp(prefix="deadpan-media-", dir="/tmp"))
    harness = Harness(work, args.sanitizers)
    exit_code = 0
    try:
        harness.execute()
    except (AssertionError, RuntimeError, OSError, ValueError, KeyError) as error:
        harness.report["status"] = "failed"
        harness.report["failure"] = str(error)
        print(str(error), file=sys.stderr)
        exit_code = 1
    harness.report["completed_utc"] = datetime.now(timezone.utc).isoformat()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(harness.report, indent=2) + "\n")
    print(f"Report: {args.output}\nFixtures: {work}\nResult: {harness.report['status']}")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
