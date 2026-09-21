#!/usr/bin/env python3
"""Reproducible, offline-device DSP candidate qualification (Python 3.12+)."""
from __future__ import annotations

import argparse
from array import array
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent
USER_AGENT = "OpenAI File Downloader, XaiImageApiFetch/1.0"
RATE = 48000
# Adapter acceptance targets declared before the first experiment. Failures are
# retained, not widened into library guarantees or made into expected passes.
TARGETS = {
    "absolute_peak_max": 2.0,
    "tone_pitch_cents_max": 15.0,
    "channel_ratio_error_db_max": 1.0,
    "dynamic_ratio_error_db_max": 1.0,
    "identity_normalized_rms_max": 0.001,
    "partition_normalized_rms_max": 0.0001,
    "local_seek_normalized_rms_max": 0.0001,
    "reset_normalized_rms_max": 0.0000001,
    "silence_peak_max": 0.000000000001,
    "isolated_channel_peak_max": 0.00000001,
    "transient_peak_error_samples_max": 1440,
    "steady_cpp_new_calls_max": 0,
}


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def command(argv: list[str], commands: list[dict], timeout: int = 120) -> str:
    started = time.perf_counter()
    result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, check=False)
    record = {
        "argv": argv, "exit_code": result.returncode,
        "elapsed_seconds": time.perf_counter() - started,
        "stdout": result.stdout, "stderr": result.stderr,
    }
    commands.append(record)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}): {argv[0]}: {result.stderr[-2000:]}")
    return result.stdout.strip()


def download(name: str, pin: dict, work: Path, cache: Path | None) -> Path:
    archive = work / f"{name}.tar.gz"
    cached = cache / archive.name if cache else None
    if cached and cached.is_file():
        shutil.copyfile(cached, archive)
    else:
        request = urllib.request.Request(pin["archive_url"], headers={"User-Agent": USER_AGENT})
        with urllib.request.urlopen(request, timeout=120) as response, archive.open("wb") as stream:
            total = 0
            while chunk := response.read(1024 * 1024):
                total += len(chunk)
                if total > 32 * 1024 * 1024:
                    raise RuntimeError("Source archive exceeded 32 MiB bound")
                stream.write(chunk)
    if sha256(archive) != pin["archive_sha256"]:
        raise RuntimeError(f"{name}: archive checksum mismatch")
    with tarfile.open(archive, "r:gz") as bundle:
        bundle.extractall(work, filter="data")
    source = work / pin["directory"]
    if not source.is_dir():
        raise RuntimeError(f"{name}: expected source root absent")
    return source


def read_pcm(path: Path, frames: int) -> array:
    payload = path.read_bytes()
    if len(payload) != frames * 2 * 4:
        raise ValueError(f"{path.name}: expected {frames} interleaved stereo float frames")
    samples = array("f")
    samples.frombytes(payload)
    if sys.byteorder != "little":
        samples.byteswap()
    if not all(math.isfinite(sample) for sample in samples):
        raise ValueError(f"{path.name}: nonfinite/unwritten sample")
    return samples


def window(samples: array, channel: int, start: float, end: float, speed: float) -> list[float]:
    first, last = round(start * RATE / speed), round(end * RATE / speed)
    if not 0 <= first < last <= len(samples) // 2:
        raise ValueError("Analysis window outside output")
    return list(samples[2 * first + channel:2 * last:2])


def rms(samples) -> float:
    return math.sqrt(math.fsum(sample * sample for sample in samples) / len(samples))


def decibels_ratio(numerator: float, denominator: float) -> float:
    return 20 * math.log10(max(numerator, 1e-30) / max(denominator, 1e-30))


def frequency(samples: list[float]) -> float:
    crossings = [
        n - 1 + (-samples[n - 1]) / (samples[n] - samples[n - 1])
        for n in range(1, len(samples)) if samples[n - 1] <= 0 < samples[n]
    ]
    if len(crossings) < 3:
        raise ValueError("Too few positive zero crossings for frequency estimate")
    return (len(crossings) - 1) * RATE / (crossings[-1] - crossings[0])


def normalized_difference(reference: array, actual: array) -> float:
    if len(reference) != len(actual):
        raise ValueError("Comparison lengths differ")
    return rms([a - b for a, b in zip(reference, actual)]) / max(rms(reference), 1e-15)


def inspect_pcm(path: Path, frames: int, assets: list[dict], assertions: list[dict]) -> array:
    samples = read_pcm(path, frames)
    peak = max(abs(sample) for sample in samples)
    assets.append({"file": path.name, "frames": frames, "sha256": sha256(path), "peak": peak})
    check(assertions, "finite_exact_duration_and_bound", path.name,
          peak <= TARGETS["absolute_peak_max"], peak, TARGETS["absolute_peak_max"])
    return samples


def check(assertions: list[dict], category: str, case: str, passed: bool, measured, limit) -> None:
    assertions.append({"category": category, "case": case, "passed": bool(passed), "measured": measured, "target": limit})


def check_phase_allocations(assertions: list[dict], case: dict) -> None:
    """Check every measured render/reset phase after engine configuration."""
    limit = TARGETS["steady_cpp_new_calls_max"]

    def phase(category: str, label: str, stats: dict) -> None:
        allocations = stats["new_calls"]
        check(assertions, category, f"{case['name']}/{label}", allocations <= limit, allocations, limit)

    phase("exact_cpp_new", "exact", case["exact"])
    phase("reset_cpp_new", "reset_call", case["reset_call"])
    categories = {"seek": "preroll_cpp_new", "process": "steady_processing_cpp_new", "flush": "flush_cpp_new"}
    for mode in ("blocks", "alternate_blocks", "reset_render"):
        for operation, category in categories.items():
            phase(category, f"{mode}/{operation}", case[mode][operation])
    for sought in case["seeks"]:
        phase("local_seek_render_cpp_new", f"local-seek-{sought['target_input_sample']}", sought["render"])


def analyze(native: dict, pcm: Path) -> dict:
    assets, assertions, measurements = [], [], []
    original = inspect_pcm(pcm / "input.f32", native["input_frames"], assets, assertions)
    for case in native["cases"]:
        name, count = case["name"], case["output_frames"]
        numerator, denominator = case["speed"]
        speed = numerator / denominator
        check(assertions, "authored_output_duration", name,
              count * numerator == native["input_frames"] * denominator,
              count, {"input_frames": native["input_frames"], "speed": case["speed"]})
        exact = inspect_pcm(pcm / f"{name}-exact.f32", count, assets, assertions)
        for mode in ("blocks", "alternate", "reset"):
            inspect_pcm(pcm / f"{name}-{mode}.f32", count, assets, assertions)
        values = {"name": name, "tones": [], "channel_ratios": [], "dynamics": [], "chirps": [], "transients": []}
        pitch_ratio = 2 ** (case["pitch_semitones"] / 12)
        for channel, base_hz in enumerate((440, 660)):
            loud = window(exact, channel, .45, .8, speed)
            quiet = window(exact, channel, 3.40, 3.60, speed)
            actual_hz = frequency(loud)
            expected_hz = base_hz * pitch_ratio
            cents = 1200 * math.log2(actual_hz / expected_hz)
            values["tones"].append({"channel": channel, "expected_hz": expected_hz, "measured_hz": actual_hz, "error_cents": cents})
            check(assertions, "independent_pitch", f"{name}/channel-{channel}", abs(cents) <= TARGETS["tone_pitch_cents_max"], cents, TARGETS["tone_pitch_cents_max"])
            dynamics = decibels_ratio(rms(quiet), rms(loud))
            values["dynamics"].append({"channel": channel, "quiet_to_loud_db": dynamics, "expected_db": -20})
            check(assertions, "preserve_authored_dynamics", f"{name}/channel-{channel}", abs(dynamics + 20) <= TARGETS["dynamic_ratio_error_db_max"], dynamics, {"expected_db": -20, "tolerance_db": TARGETS["dynamic_ratio_error_db_max"]})
            chirp_hz = frequency(window(exact, channel, 1.58, 1.62, speed))
            values["chirps"].append({"channel": channel, "mean_zero_crossing_hz": chirp_hz, "authored_center_hz_times_pitch": (1040 if channel == 0 else 620) * pitch_ratio})
            # Local peak and 90% energy width are diagnostics of transient smear.
            expected = round((2.25 if channel == 0 else 2.3) * RATE / speed)
            first, last = max(0, expected - round(.15 * RATE / speed)), min(count, expected + round(.15 * RATE / speed))
            local = list(exact[2 * first + channel:2 * last:2])
            peak = max(range(len(local)), key=lambda n: abs(local[n]))
            energy = math.fsum(sample * sample for sample in local)
            cumulative, lower, upper = 0.0, None, None
            for n, sample in enumerate(local):
                cumulative += sample * sample
                if lower is None and cumulative >= .05 * energy:
                    lower = n
                if upper is None and cumulative >= .95 * energy:
                    upper = n
            error = first + peak - expected
            values["transients"].append({"channel": channel, "peak_error_samples": error, "peak_amplitude": local[peak], "energy_90_percent_width_samples": upper - lower})
            check(assertions, "transient_position", f"{name}/channel-{channel}", abs(error) <= TARGETS["transient_peak_error_samples_max"], error, TARGETS["transient_peak_error_samples_max"])
        for start, end, ratio in ((.45, .8, .25), (2.65, 2.85, 4.0)):
            expected_db = 20 * math.log10(ratio)
            actual_db = decibels_ratio(rms(window(exact, 1, start, end, speed)), rms(window(exact, 0, start, end, speed)))
            values["channel_ratios"].append({"source_window_seconds": [start, end], "expected_db": expected_db, "measured_db": actual_db})
            check(assertions, "preserve_channel_levels", f"{name}/{start}", abs(actual_db - expected_db) <= TARGETS["channel_ratio_error_db_max"], actual_db, {"expected_db": expected_db, "tolerance_db": TARGETS["channel_ratio_error_db_max"]})
        values["embedded_silence_rms"] = [rms(window(exact, c, 1.10, 1.15, speed)) for c in (0, 1)]
        for category in ("exact_vs_blocks", "blocks_vs_alternate", "blocks_vs_reset"):
            limit = TARGETS["reset_normalized_rms_max" if category == "blocks_vs_reset" else "partition_normalized_rms_max"]
            error = case[category]["normalized_rms_error"]
            check(assertions, category, name, error <= limit, error, limit)
        check_phase_allocations(assertions, case)
        for index, sought in enumerate(case["seeks"]):
            inspect_pcm(pcm / f"{name}-seek-{index}.f32", sought["compared_output_frames"], assets, assertions)
            error = sought["difference"]["normalized_rms_error"]
            check(assertions, "local_preroll_seek_equivalence", f"{name}/target-{sought['target_input_sample']}", error <= TARGETS["local_seek_normalized_rms_max"], error, TARGETS["local_seek_normalized_rms_max"])
        if numerator == denominator and case["pitch_semitones"] == 0:
            error = normalized_difference(original, exact)
            check(assertions, "unity_identity", name, error <= TARGETS["identity_normalized_rms_max"], error, TARGETS["identity_normalized_rms_max"])
        values["exact_processing_seconds_per_output_second"] = case["exact"]["milliseconds"] / 1000 / (count / RATE)
        measurements.append(values)
    impulses = native["latency_impulses"]
    inspect_pcm(pcm / "impulse-raw.f32", impulses["raw_output_frames"], assets, assertions)
    inspect_pcm(pcm / "impulse-aligned.f32", RATE, assets, assertions)
    for channel in (0, 1):
        raw_latency = impulses["raw_peaks"][channel] - impulses["input_peaks"][channel]
        expected = native["input_latency"] + native["output_latency"]
        check(assertions, "raw_latency_accounting", str(channel), raw_latency == expected, raw_latency, expected)
        check(assertions, "preroll_flush_alignment", str(channel), impulses["aligned_peaks"][channel] == impulses["input_peaks"][channel], impulses["aligned_peaks"][channel], impulses["input_peaks"][channel])
    silent = inspect_pcm(pcm / "silence.f32", native["input_frames"], assets, assertions)
    peak = max(abs(sample) for sample in silent)
    check(assertions, "full_silence", "pitch+7", peak <= TARGETS["silence_peak_max"], peak, TARGETS["silence_peak_max"])
    isolated = inspect_pcm(pcm / "left-only.f32", native["input_frames"], assets, assertions)
    peak = max(abs(sample) for sample in isolated[1::2])
    check(assertions, "isolated_channel_leakage", "pitch+7", peak <= TARGETS["isolated_channel_peak_max"], peak, TARGETS["isolated_channel_peak_max"])
    short = inspect_pcm(pcm / "short-output.f32", native["short_input"]["output_frames"], assets, assertions)
    check(assertions, "short_clip_supported", "1003-sample-clip", native["short_input"]["exact_success"], native["short_input"]["exact_success"], True)
    for index, control in enumerate(native["continuous_controls"]):
        for mode in ("exact", "blocks"):
            inspect_pcm(pcm / f"continuous-{index}-{mode}.f32", control["output_frames"], assets, assertions)
        error = control["exact_vs_blocks"]["normalized_rms_error"]
        check(assertions, "continuous_control_partition", str(index), error <= TARGETS["partition_normalized_rms_max"], error, TARGETS["partition_normalized_rms_max"])
    categories = {}
    for assertion in assertions:
        group = categories.setdefault(assertion["category"], {"passed": 0, "failed": 0})
        group["passed" if assertion["passed"] else "failed"] += 1
    return {
        "targets": TARGETS, "measurements": measurements, "assets": assets,
        "assertions": assertions, "categories": categories,
        "assertions_passed": sum(value["passed"] for value in assertions),
        "assertions_failed": sum(not value["passed"] for value in assertions),
        "short_output_peak": max(abs(sample) for sample in short),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="JSON report destination")
    parser.add_argument("--download-cache", type=Path, help="Optional immutable stretch.tar.gz and linear.tar.gz cache")
    parser.add_argument("--sanitizers", action="store_true", help="Instrument probe and header-only DSP with ASan and UBSan")
    args = parser.parse_args()
    if sys.version_info < (3, 12):
        parser.error("Python 3.12+ is required for the safe tar extraction filter")
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("This measured build configuration requires Apple Silicon macOS")
    work = Path(tempfile.mkdtemp(prefix="deadpan-audio-", dir="/tmp"))
    report = {
        "schema_version": 1, "created_utc": datetime.now(timezone.utc).isoformat(),
        "work_directory": str(work), "sanitizers": args.sanitizers,
        "python_version": sys.version, "platform": platform.platform(),
        "harness_sha256": {name: sha256(ROOT / name) for name in ("run.py", "probe.cpp", "pins.json", "test_analysis.py")},
        "commands": [], "status": "incomplete",
    }
    code = 2
    try:
        commands = report["commands"]
        report["host"] = {name: command(argv, commands) for name, argv in {
            "os": ["sw_vers"], "cpu": ["sysctl", "-n", "machdep.cpu.brand_string"],
            "memory_bytes": ["sysctl", "-n", "hw.memsize"], "machine_model": ["sysctl", "-n", "hw.model"],
            "compiler": ["clang++", "--version"], "sdk_version": ["xcrun", "--show-sdk-version"],
            "sdk_path": ["xcrun", "--show-sdk-path"],
        }.items()}
        pins = json.loads((ROOT / "pins.json").read_text())
        sources = {name: download(name, pin, work, args.download_cache) for name, pin in pins.items()}
        report["sources"] = {
            name: {
                **pins[name], "path": str(path), "license_text": (path / "LICENSE.txt").read_text(),
                "license_sha256": sha256(path / "LICENSE.txt"),
                "header_sha256": {str(header.relative_to(path)): sha256(header) for header in sorted(path.rglob("*.h"))},
            } for name, path in sources.items()
        }
        executable = work / "audio_probe"
        flags = ["-std=c++17", "-O2", "-g", "-Wall", "-Wextra", "-Werror", "-arch", "arm64", "-mmacosx-version-min=15.0"]
        if args.sanitizers:
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-fno-sanitize-recover=all"]
        includes = [argument for source in sources.values() for argument in ("-isystem", str(source / "include"))]
        command(["clang++", *flags, *includes, str(ROOT / "probe.cpp"), "-o", str(executable)], commands, 180)
        report["build"] = {"flags": flags, "fft_backend": "Signalsmith Linear portable built-in FFT; no external FFT defines", "binary_sha256": sha256(executable), "dynamic_libraries": command(["otool", "-L", str(executable)], commands), "load_commands": command(["otool", "-l", str(executable)], commands)}
        pcm = work / "pcm"
        pcm.mkdir()
        print(f"Running native probe; retained PCM: {pcm}", flush=True)
        native_text = command([str(executable), str(pcm)], commands, 300)
        (work / "native.json").write_text(native_text + "\n")
        native = json.loads(native_text)
        report["native"] = native
        report["analysis"] = analyze(native, pcm)
        failed = report["analysis"]["assertions_failed"]
        report["status"] = "candidate_targets_failed" if failed else "candidate_targets_passed"
        code = int(bool(failed))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        report["status"] = "harness_error"
        report["error"] = f"{type(error).__name__}: {error}"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(json.dumps({"report": str(args.output), "status": report["status"], "exit_code": code,
                      "passed": report.get("analysis", {}).get("assertions_passed"),
                      "failed": report.get("analysis", {}).get("assertions_failed"),
                      "error": report.get("error")}, indent=2))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
