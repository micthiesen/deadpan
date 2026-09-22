"""Analytic oracle checks and independent 80-digit counterexample witnesses."""
import json
import math
import platform
import sys
from pathlib import Path

import mpmath as mp
import numpy as np

import oracle

ROOT = Path(__file__).resolve().parent


def main():
    # Known finite sinc sums, including cancellation and interpolation at the
    # exact integer samples, independently exercise sign and coordinate rules.
    pair = oracle.Direct(np.array([1.0, 1.0]))
    assert abs(pair.value(0.5, True) - 4.0 / math.pi) < 1e-15
    cancelled = oracle.Direct(np.array([1.0, -1.0]))
    assert cancelled.value(0.5, True) == 0.0
    impulse = oracle.Direct(np.array([1.0]))
    assert impulse.value(0.0, True) == 1.0
    assert impulse.value(1.0, True) == 0.0
    assert abs(impulse.value(0.5, True) - 2.0 / math.pi) < 1e-15
    assert abs(impulse.value(-0.5, True) - 2.0 / math.pi) < 1e-15

    rng = np.random.default_rng(20876)
    samples = rng.uniform(-1.0, 1.0, 17)
    direct = oracle.Direct(samples)
    finite_sum_residual = 0.0
    for point in np.linspace(-19.125, 34.375, 221):
        independent = math.fsum(samples * np.sinc(point - np.arange(len(samples))))
        finite_sum_residual = max(finite_sum_residual, abs(independent - direct.value(float(point), True)))
    assert finite_sum_residual < 1e-13
    small = oracle.audit_channel(samples, 128, 32, 16)
    assert small["fft_direct_max_observed_difference"] < 1e-13
    assert small["refined_peak"] <= small["qualified_numerical_global_upper"]

    results = json.loads((ROOT / "results.json").read_text())
    mp.mp.dps = 80
    witnesses = []
    for case in results["results"]:
        if not case["exceeds_minus_1_dbtp"]:
            continue
        channel, report = max(enumerate(case["channels"]), key=lambda item: item[1]["refined_peak"])
        samples = oracle.load_wave(ROOT / "inputs" / (case["case"] + ".wav"))[:, channel]
        coordinate = mp.mpf(report["refined_coordinate"])
        integer = int(mp.floor(coordinate))
        phase = coordinate - integer
        # Float32 PCM converts to float64 exactly, then mp.mpf(float) retains
        # that binary value exactly at this precision. Every sample is included.
        total = mp.fsum(
            mp.mpf(float(value)) * (-1 if index % 2 else 1) / (coordinate - index)
            for index, value in enumerate(samples)
        )
        value = (-1 if integer % 2 else 1) * mp.sin(mp.pi * phase) / mp.pi * total
        magnitude = abs(value)
        dbtp = 20 * mp.log10(magnitude)
        ceiling = mp.power(10, -mp.mpf(1) / 20)
        assert magnitude > ceiling
        assert abs(float(value) - report["refined_signed_value"]) < 1e-12
        witnesses.append({
            "case": case["case"], "channel": channel,
            "wav_sha256": case["wav_sha256"],
            "coordinate_exact_binary_float": float(coordinate).hex(),
            "coordinate_decimal": mp.nstr(coordinate, 80),
            "sinc_sum": mp.nstr(value, 75),
            "magnitude": mp.nstr(magnitude, 75),
            "dbtp": mp.nstr(dbtp, 75),
            "above_ceiling_amplitude": mp.nstr(magnitude - ceiling, 75),
            "float64_difference": abs(float(value) - report["refined_signed_value"]),
        })
    output = {
        "python": sys.version, "platform": platform.platform(),
        "numpy": np.__version__, "mpmath": mp.__version__, "decimal_digits": mp.mp.dps,
        "check_script_sha256": oracle.sha256(Path(__file__)),
        "finite_direct_vs_numpy_sinc_max_difference": finite_sum_residual,
        "small_fft_vs_direct_max_difference": small["fft_direct_max_observed_difference"],
        "analytic_cases_passed": 6,
        "witnesses": witnesses,
        "limits": "80-digit ordinary arithmetic, not interval-certified maxima; witnesses suffice to disprove ceiling",
    }
    oracle.write_json(ROOT / "checks.json", output)
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
