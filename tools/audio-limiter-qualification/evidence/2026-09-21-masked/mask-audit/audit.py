"""Full finite sinc audit of three existing alternating-mask WAV files."""
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
SOURCE = Path("/tmp/deadpan-joint-limiter-20260921/mask-stress")
CEILING = 10.0 ** (-1.0 / 20.0)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(name, value):
    (ROOT / name).write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def precise(samples, coordinate):
    position = mp.mpf(coordinate)
    integer = int(mp.floor(position))
    phase = position - integer
    if phase == 0:
        return mp.mpf(float(samples[integer])) if 0 <= integer < len(samples) else mp.mpf(0)
    total = mp.fsum(mp.mpf(float(value)) * (-1 if n % 2 else 1) / (position - n)
                    for n, value in enumerate(samples))
    return (-1 if integer % 2 else 1) * mp.sin(mp.pi * phase) / mp.pi * total


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reuse-inputs", action="store_true")
    args = parser.parse_args()
    inputs = ROOT / "inputs"
    inputs.mkdir(exist_ok=True)
    if not args.reuse_inputs:
        shutil.copyfile(SOURCE / "results.json", inputs / "parent-results.json")
    source = json.loads((inputs / "parent-results.json").read_text())
    assert len(source["results"]) == 3
    for case in source["results"]:
        filename = case["case"] + ".wav"
        if not args.reuse_inputs:
            shutil.copyfile(SOURCE / filename, inputs / filename)
        assert sha(inputs / filename) == case["output_sha256"], filename
    environment = {
        "python": sys.version, "platform": platform.platform(), "numpy": np.__version__,
        "mpmath": mp.__version__, "argv": sys.argv,
        "script_sha256": sha(Path(__file__)), "sinc_helper_sha256": sha(ROOT / "finite_sinc.py"),
        "parent_results_sha256": sha(inputs / "parent-results.json"),
        "verified_wav_hashes": 3, "reconstruction": "Complete finite zero-extended sinc; no window or fixed radius",
        "phases": 128, "refinements_per_channel": 16,
        "limits": "Fixed output witnesses and ordinary numerical bounds, not interval-certified maxima or universal limiter qualification",
    }
    write_json("environment.json", environment)
    mp.mp.dps = 80
    results = []
    checks = []
    start = time.monotonic()
    for case in source["results"]:
        pcm = finite_sinc.load_wave(inputs / (case["case"] + ".wav"))
        assert float(np.max(np.abs(pcm))) == case["sample_peak"]
        zero_parities = [parity for parity in [0, 1] if np.all(pcm[parity::2] == 0)]
        assert len(zero_parities) == 1
        l1 = max(math.fsum(np.abs(pcm[:, ch])) for ch in range(2))
        required = max(2, math.ceil(l1 / (math.pi * CEILING)) + 1)
        distance = 1 << (required - 1).bit_length()
        channels = [finite_sinc.audit_channel(pcm[:, ch], 128, distance, 16) for ch in range(2)]
        for channel, report in enumerate(channels):
            value = precise(pcm[:, channel], report["refined_coordinate"])
            assert abs(float(value) - report["refined_signed_value"]) < 1e-12
            check = {
                "case": case["case"], "channel": channel, "wav_sha256": case["output_sha256"],
                "coordinate_exact_binary_float": report["refined_coordinate"].hex(),
                "coordinate_decimal": mp.nstr(mp.mpf(report["refined_coordinate"]), 80),
                "signed_value": mp.nstr(value, 75), "magnitude": mp.nstr(abs(value), 75),
                "dbtp": mp.nstr(20 * mp.log10(abs(value)), 75),
                "exceeds_minus_1_dbtp": bool(abs(value) > mp.power(10, -mp.mpf(1)/20)),
                "float64_difference": abs(float(value) - report["refined_signed_value"]),
            }
            checks.append(check)
        item = {
            "case": case["case"], "wav_sha256": case["output_sha256"], "hash_verified": True,
            "zero_frame_parity": zero_parities[0], "frames_verified_zero": len(pcm[zero_parities[0]::2]),
            "channels": channels,
            "worst_refined_peak": max(c["refined_peak"] for c in channels),
            "worst_refined_dbtp": max(c["refined_dbtp"] for c in channels),
            "exceeds_minus_1_dbtp": any(c["exceeds_minus_1_dbtp"] for c in channels),
            "numerical_global_upper": max(c["qualified_numerical_global_upper"] for c in channels),
        }
        results.append(item)
        write_json("results.json", {"environment": environment, "results": results,
                   "seconds": time.monotonic() - start})
        write_json("high-precision.json", {"decimal_digits": 80, "checks": checks})
        print(json.dumps({"case": item["case"], "peak": item["worst_refined_peak"],
              "dbtp": item["worst_refined_dbtp"], "exceeds": item["exceeds_minus_1_dbtp"],
              "upper": item["numerical_global_upper"]}), flush=True)


if __name__ == "__main__":
    main()
