"""Read-only audit of retained joint-limiter WAV outputs; never regenerates PCM."""
import argparse
import hashlib
import json
import math
import platform
import shutil
import sys
import time
from pathlib import Path

import mpmath as mp
import numpy as np

import finite_sinc

ROOT = Path(__file__).resolve().parent
PARENT = Path("/tmp/deadpan-joint-limiter-20260921")
MASKS = {"full": [], "gap": [(3072, 3584)],
         "tiny-gaps": [(2048, 2049), (4096, 4098), (6000, 6017)],
         "silent": [(0, 8192)]}
LIMIT = 10.0 ** (-1.0 / 20.0)


def write_json(name, data):
    (ROOT / name).write_text(json.dumps(data, indent=2, allow_nan=False) + "\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def silent_channel(samples):
    assert np.all(samples == 0)
    return {
        "exact_silence": True, "negative_zero_samples": int(np.sum(np.signbit(samples))),
        "sample_peak": 0.0, "sample_l1": 0.0, "grid_peak": 0.0,
        "refined_peak": 0.0, "refined_dbtp": None, "refined_coordinate": None,
        "refined_signed_value": 0.0, "exceeds_minus_1_dbtp": False,
        "qualified_numerical_global_upper": 0.0,
        "fft_direct_max_observed_difference": 0.0,
        "method": "All input values, including negative zeros, equal zero exactly; every sinc sum is identically zero.",
    }


def high_precision(case, channel):
    samples = finite_sinc.load_wave(ROOT / "inputs" / (case["case"] + ".wav"))[:, channel]
    record = case["channels"][channel]
    coordinate = mp.mpf(record["refined_coordinate"])
    integer = int(mp.floor(coordinate))
    phase = coordinate - integer
    if phase == 0:
        value = mp.mpf(float(samples[integer])) if 0 <= integer < len(samples) else mp.mpf(0)
    else:
        total = mp.fsum(mp.mpf(float(value)) * (-1 if n % 2 else 1) / (coordinate - n)
                        for n, value in enumerate(samples))
        value = (-1 if integer % 2 else 1) * mp.sin(mp.pi * phase) / mp.pi * total
    ceiling = mp.power(10, -mp.mpf(1) / 20)
    assert abs(float(value) - record["refined_signed_value"]) < 1e-12
    return {
        "case": case["case"], "channel": channel, "wav_sha256": case["wav_sha256"],
        "coordinate_exact_binary_float": float(coordinate).hex(),
        "coordinate_decimal": mp.nstr(coordinate, 80),
        "signed_value": mp.nstr(value, 75), "magnitude": mp.nstr(abs(value), 75),
        "dbtp": mp.nstr(20 * mp.log10(abs(value)), 75) if value else None,
        "ceiling_minus_magnitude": mp.nstr(ceiling - abs(value), 75),
        "exceeds_minus_1_dbtp": bool(abs(value) > ceiling),
        "float64_difference": abs(float(value) - record["refined_signed_value"]),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reuse-inputs", action="store_true")
    args = parser.parse_args()
    inputs = ROOT / "inputs"
    inputs.mkdir(exist_ok=True)
    if not args.reuse_inputs:
        shutil.copyfile(PARENT / "run-8192" / "results.json", inputs / "parent-results.json")
        shutil.copyfile(PARENT / "prototype.py", inputs / "parent-prototype.py")
    source = json.loads((inputs / "parent-results.json").read_text())
    assert len(source["results"]) == 42
    assert source["script_sha256"] == sha(inputs / "parent-prototype.py")
    for case in source["results"]:
        name = case["case"]
        if not args.reuse_inputs:
            shutil.copyfile(PARENT / "run-8192" / (name + ".wav"), inputs / (name + ".wav"))
            shutil.copyfile(PARENT / "run-8192" / (name + "-meter.json"), inputs / (name + "-meter.json"))
        assert sha(inputs / (name + ".wav")) == case["output_sha256"], name
    environment = {
        "python": sys.version, "platform": platform.platform(), "numpy": np.__version__,
        "mpmath": mp.__version__, "argv": sys.argv,
        "audit_script_sha256": sha(Path(__file__)), "finite_sinc_script_sha256": sha(ROOT / "finite_sinc.py"),
        "parent_results_sha256": sha(inputs / "parent-results.json"),
        "parent_prototype_sha256": sha(inputs / "parent-prototype.py"),
        "conditioner_sha256": source["conditioner_sha256"],
        "parent_attack": source["attack"], "parent_release": source["release"],
        "hash_verified_wavs": 42,
        "interpolation": "complete finite-sequence unwindowed sinc; integer samples outside stored PCM are zero",
        "fractional_phases": 128, "local_maxima_refined_per_channel": 16,
        "arithmetic": "float64 FFT + direct compensated sums; 80-digit checks for every failing channel or the worst witness if none fail",
        "limits": "Bounds use ordinary float64, not interval-certified arithmetic. Fixed-fixture evidence only, no limiter or arbitrary-input qualification.",
    }
    write_json("environment.json", environment)
    # Preserve the known independent analytic oracle checks in this run.
    assert abs(finite_sinc.Direct(np.array([1.0, 1.0])).value(0.5, True) - 4 / math.pi) < 1e-15
    assert finite_sinc.Direct(np.array([1.0, -1.0])).value(0.5, True) == 0
    start = time.monotonic()
    results = []
    for source_case in source["results"]:
        name = source_case["case"]
        pcm = finite_sinc.load_wave(inputs / (name + ".wav"))
        assert float(np.max(np.abs(pcm))) == source_case["sample_peak"]
        masked_frames = 0
        for low, high in MASKS[source_case["mask"]]:
            assert np.all(pcm[low:high] == 0), (name, low, high)
            masked_frames += high - low
        l1 = max(math.fsum(np.abs(pcm[:, channel])) for channel in range(2))
        needed = max(2, math.ceil(l1 / (math.pi * LIMIT)) + 1)
        distance = 1 << (needed - 1).bit_length()
        channels = [silent_channel(pcm[:, ch]) if np.all(pcm[:, ch] == 0)
                    else finite_sinc.audit_channel(pcm[:, ch], 128, distance, 16)
                    for ch in range(2)]
        peak = max(channel["refined_peak"] for channel in channels)
        item = {
            "case": name, "mask": source_case["mask"], "frames": len(pcm),
            "wav_sha256": source_case["output_sha256"], "hash_verified": True,
            "masked_frames_verified_zero": masked_frames,
            "all_frames_zero": bool(np.all(pcm == 0)),
            "bs1770_dbtp": source_case["bs1770_dbtp"], "channels": channels,
            "worst_refined_peak": peak,
            "worst_refined_dbtp": 20 * math.log10(peak) if peak else None,
            "exceeds_minus_1_dbtp": peak > LIMIT,
            "qualified_numerical_global_upper": max(c["qualified_numerical_global_upper"] for c in channels),
        }
        results.append(item)
        write_json("results.json", {"environment": environment, "results": results,
                   "seconds": time.monotonic() - start})
        print(json.dumps({"case": name, "dbtp": item["worst_refined_dbtp"],
              "exceeds": item["exceeds_minus_1_dbtp"], "mask_zero_frames": masked_frames}), flush=True)
    failing = [(case, ch) for case in results for ch, c in enumerate(case["channels"])
               if c["exceeds_minus_1_dbtp"]]
    if not failing:
        failing = [max(((case, ch) for case in results for ch in range(2)),
                       key=lambda item: item[0]["channels"][item[1]]["refined_peak"])]
    mp.mp.dps = 80
    checks = [high_precision(case, ch) for case, ch in failing]
    write_json("high-precision.json", {"decimal_digits": 80, "mpmath": mp.__version__, "checks": checks})
    summary = {
        "fixtures": len(results), "channels": len(results) * 2,
        "hash_verified": 42, "ceiling_linear": LIMIT,
        "failing_fixtures": [case["case"] for case in results if case["exceeds_minus_1_dbtp"]],
        "fully_silent_fixtures": [case["case"] for case in results if case["all_frames_zero"]],
        "worst_fixture": max(results, key=lambda case: case["worst_refined_peak"])["case"],
        "worst_refined_peak": max(case["worst_refined_peak"] for case in results),
        "worst_refined_dbtp": max(case["worst_refined_dbtp"] for case in results if case["worst_refined_dbtp"] is not None),
        "all_qualified_numerical_global_upper_bounds_below_ceiling": all(case["qualified_numerical_global_upper"] < LIMIT for case in results),
        "largest_qualified_numerical_global_upper": max(case["qualified_numerical_global_upper"] for case in results),
        "max_observed_fft_direct_difference": max(channel["fft_direct_max_observed_difference"] for case in results for channel in case["channels"]),
        "seconds": time.monotonic() - start,
    }
    write_json("summary.json", summary)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
