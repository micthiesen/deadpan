"""Unmasked LTI kernel moments only; no gain constraints or limiter design."""
import hashlib
import json
import math
import re
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent
SOURCE = Path("/Users/michael/Code/deadpan/crates/deadpan-audio/src/true_peak.rs")


def measure(coefficients, offsets, phase):
    absolute = np.abs(coefficients)
    return {
        "phase_coordinate": phase,
        "coefficient_count": len(coefficients),
        "support_integer_offsets": [int(offsets[0]), int(offsets[-1])],
        "sum": math.fsum(coefficients),
        "l1": math.fsum(absolute),
        "integer_origin_distance_sum": math.fsum(absolute * np.abs(offsets)),
        "phase_coordinate_distance_sum": math.fsum(absolute * np.abs(offsets - phase)),
    }


def family(name, conditioner, kernels, offsets, phases):
    center = len(conditioner) // 2
    output = []
    for kernel, phase in zip(kernels, phases):
        combined = np.convolve(conditioner, kernel)
        combined_offsets = np.arange(int(offsets[0]) - center, int(offsets[-1]) + center + 1)
        assert len(combined_offsets) == len(combined)
        output.append({"raw": measure(kernel, offsets, phase),
                       "conditioned": measure(combined, combined_offsets, phase)})
    return {
        "name": name, "phases": output,
        "max_l1": max(item["conditioned"]["l1"] for item in output),
        "max_integer_origin_distance_sum": max(item["conditioned"]["integer_origin_distance_sum"] for item in output),
        "max_phase_coordinate_distance_sum": max(item["conditioned"]["phase_coordinate_distance_sum"] for item in output),
    }


def main():
    text = SOURCE.read_text()
    literal = re.search(r"const FIR:.*?= (\[.*?\n\]);", text, re.S).group(1)
    table = np.array(json.loads(re.sub(r",(\s*\])", r"\1", literal)), dtype=np.float64)
    assert table.shape == (12, 4)
    # Oldest-to-newest source coordinates, anchored five frames after the
    # oldest tap: nominal phase centers 7/8,5/8,3/8,1/8 respectively.
    bs_offsets = np.arange(-5, 7)
    bs_phases = [7/8, 5/8, 3/8, 1/8]
    radius = 256
    long_offsets = np.arange(1-radius, radius+1)
    long_phases = np.arange(8) / 8
    long_kernels = []
    for phase in long_phases:
        delta = phase - long_offsets
        window = np.i0(10 * np.sqrt(np.maximum(0, 1 - (delta/radius)**2))) / np.i0(10)
        kernel = np.where(np.abs(delta) < radius, np.sinc(delta) * window, 0)
        kernel /= sum(kernel)
        if phase == 0:
            kernel[:] = 0
            kernel[radius - 1] = 1
        long_kernels.append(kernel)
    results = []
    for name in ["kaiser255-beta16", "kaiser511-beta16"]:
        data = json.loads((ROOT / (name + ".json")).read_text())
        coefficients = np.array(data["coefficients_f64"])
        center = len(coefficients) // 2
        item = {
            "name": name, "coefficient_sha256": data["raw_f64le_sha256"],
            "conditioner_only": measure(coefficients, np.arange(-center, center+1), 0.0),
            "bs1770": family("BS.1770-5 Annex 2, four phases", coefficients, table.T, bs_offsets, bs_phases),
            "kaiser256": family("radius256 beta10 eight phases", coefficients, long_kernels, long_offsets, long_phases),
        }
        results.append(item)
    output = {
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "source_peak_meter_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        "source_peak_meter_path": str(SOURCE),
        "bs1770_table_rows_oldest_to_newest": table.tolist(),
        "long_kernel_parameters": {"radius": 256, "beta": 10, "phases": 8,
                                   "normalization": "each phase sums to one; phase zero is an impulse"},
        "coordinate_convention": "Centered conditioner F[r], interpolation H_p[j]: C_p[l]=sum_(r+j=l) F[r] H_p[j]. This uses delay-compensated logical coordinates; a causal conditioner still delays by its integer center.",
        "distance_conventions": "D0=sum_l |C_p[l]| |l| references integer gain origin; Dp=sum_l |C_p[l]| |l-p| references phase coordinate. Both are listed to prevent mixing conventions.",
        "limits": "Full unmasked LTI convolutions only. Output masks break translation invariance. These numbers do not qualify mask-dependent kernels, gain slew, a limiter, finite arithmetic bounds, or ideal-sinc reconstruction.",
        "results": results,
    }
    (ROOT / "combined-kernels.json").write_text(json.dumps(output, indent=2, allow_nan=False) + "\n")
    for result in results:
        print(json.dumps({"name": result["name"],
              "conditioner_l1": result["conditioner_only"]["l1"],
              "conditioner_distance": result["conditioner_only"]["integer_origin_distance_sum"],
              "bs1770_l1": result["bs1770"]["max_l1"],
              "bs1770_distance": result["bs1770"]["max_integer_origin_distance_sum"],
              "kaiser256_l1": result["kaiser256"]["max_l1"],
              "kaiser256_distance": result["kaiser256"]["max_integer_origin_distance_sum"]}))


if __name__ == "__main__":
    main()
