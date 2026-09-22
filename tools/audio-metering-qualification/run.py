#!/usr/bin/env python3
"""Developer-only generated PCM qualification; no official EBU audio is copied.

Build the Rust measure_pcm example first. The reference FFmpeg executable is
external developer tooling, never an application dependency or shipped asset.
"""

import argparse
from array import array
import datetime
import hashlib
import json
import math
from pathlib import Path
import platform
import re
import resource
import shutil
import signal
import struct
import subprocess
import sys

RATE = 48_000


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write_tones(path, segments, frequency=1000, right_gain=1):
    """Segments are (integer frame count, peak dBFS or None for silence)."""
    with path.open("wb") as destination:
        for frames, level in segments:
            amplitude = 0 if level is None else 10 ** (level / 20)
            cycle = array("f")
            for n in range(RATE):
                sample = amplitude * math.sin(math.tau * frequency * n / RATE)
                cycle.extend((sample, sample * right_gain))
            if sys.byteorder != "little":
                cycle.byteswap()
            raw = cycle.tobytes()
            while frames:
                count = min(frames, RATE)
                destination.write(raw[:count * 8])
                frames -= count


def ffmpeg_reference(executable, source, log):
    command = [str(executable), "-hide_banner", "-nostats", "-f", "f32le", "-ar", str(RATE), "-ac", "2", "-i", str(source), "-af", "ebur128=peak=true:framelog=verbose", "-f", "null", "-"]
    result = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, check=True)
    log.write_text(result.stderr)
    loudness = float(re.findall(r"I:\s+(-?[\d.]+) LUFS", result.stderr)[-1])
    peak = float(re.findall(r"Peak:\s+(-?[\d.]+|-inf) dBFS", result.stderr)[-1])
    return loudness, peak


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measure", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="new scratch directory")
    parser.add_argument("--ffmpeg", default="ffmpeg")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    measure = args.measure.resolve(strict=True)
    reference = shutil.which(args.ffmpeg)
    if reference is None:
        raise SystemExit("reference FFmpeg was not found")
    reference = Path(reference).resolve(strict=True)
    version = subprocess.check_output([str(reference), "-version"], text=True)
    (args.output / "ffmpeg-version.txt").write_text(version)
    report = {
        "schema_version": 1,
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(),
        "python": sys.version,
        "measure_sha256": digest(measure),
        "reference_sha256": digest(reference),
        "reference_version": version.splitlines()[0],
        "script_sha256": digest(Path(__file__)),
        "cases": [],
        "failure_paths": [],
    }
    definitions = [
        ("ebu-1", [(20 * RATE, -23)], -23, 1000, 1),
        ("ebu-2", [(20 * RATE, -33)], -33, 1000, 1),
        ("ebu-3", [(10 * RATE, -36), (60 * RATE, -23), (10 * RATE, -36)], -23, 1000, 1),
        ("ebu-4", [(10 * RATE, -72), (10 * RATE, -36), (60 * RATE, -23), (10 * RATE, -36), (10 * RATE, -72)], -23, 1000, 1),
        ("ebu-5", [(20 * RATE, -26), (20 * RATE + RATE // 10, -20), (20 * RATE, -26)], -23, 1000, 1),
        ("inverted", [(10 * RATE, -23)], -23, 1000, -1),
        ("mono-997", [(10 * RATE, 0)], -3.01, 997, 0),
        ("low-20", [(10 * RATE, -23)], -36.96636779238207, 20, 1),
        ("high-10000", [(10 * RATE, -23)], -19.649117777429872, 10000, 1),
        ("silence", [(10 * RATE, None)], None, 1000, 1),
        ("below-gate", [(10 * RATE, -71)], None, 1000, 1),
        ("short", [(19_199, -12)], None, 1000, 1),
        ("empty", [], None, 1000, 1),
    ]
    for name, segments, expected, frequency, right_gain in definitions:
        source = args.output / (name + ".f32le")
        destination = args.output / (name + ".json")
        write_tones(source, segments, frequency, right_gain)
        subprocess.run([str(measure), str(source), str(destination)], check=True)
        result = json.loads(destination.read_text())
        reference_lufs, reference_peak = ffmpeg_reference(reference, source, args.output / (name + "-reference.log"))
        peak = max(result["peaks"]["true_peak"])
        measured_peak = 20 * math.log10(peak) if peak > 0 else -math.inf
        measured = result["loudness"]["integrated_lufs"]
        assertions = {
            "input_hash_unchanged": result["input_sha256"] == digest(source),
            "frame_counts": result["loudness"]["measured_frames"] == result["peaks"]["measured_frames"] == sum(frames for frames, _ in segments),
            "expected_integrated": measured is None if expected is None else measured is not None and abs(measured - expected) <= 0.1,
            # FFmpeg uses a -70 sentinel for an undefined integrated reading.
            "reference_integrated": reference_lufs == -70 if measured is None else abs(measured - reference_lufs) <= 0.11,
        }
        # Abrupt tone boundaries exercise different finite reconstruction
        # filters, without a published inter-filter agreement threshold. Keep
        # discrepancies visible as diagnostics; qualify true peak separately
        # against the published faded-waveform expectations below. FFmpeg's
        # empty-input 0 dBFS sentinel is not a measurement.
        applicable = result["peaks"]["measured_frames"] > 0
        difference = measured_peak - reference_peak if peak > 0 and math.isfinite(reference_peak) else None
        diagnostic = {"applicable": applicable, "difference_db": difference, "within_0_4_db": None if not applicable else abs(difference) <= 0.4 if difference is not None else measured_peak == reference_peak}
        entry = {"name": name, "segments": segments, "frequency_hz": frequency, "right_gain": right_gain, "expected_lufs": expected, "reference_lufs": reference_lufs, "reference_peak_dbfs": reference_peak if math.isfinite(reference_peak) else None, "reference_peak_diagnostic": diagnostic, "assertions": assertions, "result": result}
        report["cases"].append(entry)
        print(name, measured, reference_lufs, all(assertions.values()), flush=True)

    # Published EBU Tech 3341 Table 1 sinusoidal true-peak cases 15-19.
    # The short, faded waveforms are mathematically synthesized here.
    for case, divisor, amplitude, phase, expected in [(15, 4, .5, 0, -6), (16, 4, .5, 45, -6), (17, 6, .5, 60, -6), (18, 8, .5, 67.5, -6), (19, 4, 1.41, 45, 3)]:
        source = args.output / f"ebu-{case}.f32le"
        destination = args.output / f"ebu-{case}.json"
        with source.open("wb") as stream:
            for n in range(4800):
                fade = min(n / 480, (4799 - n) / 480, 1)
                sample = fade * amplitude * math.sin(math.tau * n / divisor + math.radians(phase))
                stream.write(struct.pack("<ff", sample, sample))
        subprocess.run([str(measure), str(source), str(destination)], check=True)
        result = json.loads(destination.read_text())
        _, reference_peak = ffmpeg_reference(reference, source, args.output / f"ebu-{case}-reference.log")
        assertions = {"peak_tolerance": all(expected - .4 <= db <= expected + .2 for db in result["peaks"]["true_peak_dbtp"]), "reference_peak_tolerance": expected - .4 <= reference_peak <= expected + .2, "input_hash_unchanged": result["input_sha256"] == digest(source), "no_complete_loudness_window": result["loudness"]["integrated_lufs"] is None}
        report["cases"].append({"name": f"ebu-{case}", "expected_dbtp": expected, "reference_peak_dbfs": reference_peak, "assertions": assertions, "result": result})
        print(f"ebu-{case}", result["peaks"]["true_peak_dbtp"], all(assertions.values()), flush=True)

    for name, raw in [("truncated", b"12345"), ("nan", struct.pack("<ff", 0, math.nan)), ("infinity", struct.pack("<ff", math.inf, 0)), ("magnitude", struct.pack("<ff", -16.01, 0))]:
        source = args.output / (name + ".f32le")
        source.write_bytes(raw)
        destination = args.output / (name + ".json")
        result = subprocess.run([str(measure), str(source), str(destination)], capture_output=True, text=True)
        report["failure_paths"].append({"name": name, "exit_code": result.returncode, "stderr": result.stderr, "passed": result.returncode != 0 and not destination.exists()})
    source = args.output / "over-budget.f32le"
    with source.open("wb") as stream:
        stream.truncate((24 * 60 * 60 * RATE + 1) * 8)  # Sparse; never read or retained.
    destination = args.output / "over-budget.json"
    result = subprocess.run([str(measure), str(source), str(destination)], capture_output=True, text=True)
    source.unlink()
    report["failure_paths"].append({"name": "over-budget", "exit_code": result.returncode, "stderr": result.stderr, "passed": result.returncode != 0 and not destination.exists()})
    destination = args.output / "empty.json"
    before = digest(destination)
    result = subprocess.run([str(measure), str(args.output / "empty.f32le"), str(destination)], capture_output=True, text=True)
    report["failure_paths"].append({"name": "no-overwrite", "exit_code": result.returncode, "stderr": result.stderr, "passed": result.returncode != 0 and digest(destination) == before})
    destination = args.output / "write-failure.json"
    before_files = set(args.output.iterdir())
    def fail_report_write():
        signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
        resource.setrlimit(resource.RLIMIT_FSIZE, (32, 32))
    result = subprocess.run([str(measure), str(args.output / "empty.f32le"), str(destination)], capture_output=True, text=True, preexec_fn=fail_report_write)
    report["failure_paths"].append({"name": "write-failure", "exit_code": result.returncode, "stderr": result.stderr, "passed": result.returncode != 0 and not destination.exists() and set(args.output.iterdir()) == before_files})
    report["passed"] = all(all(case["assertions"].values()) for case in report["cases"]) and all(case["passed"] for case in report["failure_paths"])
    report["peak_diagnostic_mismatches"] = [case["name"] for case in report["cases"] if case.get("reference_peak_diagnostic", {}).get("within_0_4_db") is False]
    report["completed_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    (args.output / "qualification.json").write_text(json.dumps(report, indent=2) + "\n")
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
