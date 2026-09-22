#!/usr/bin/env python3
"""Independent finite-sequence, zero-extended sinc reconstruction audit.

No limiter code or Kaiser/windowed interpolation is used. All N samples enter
each queried point: f(t) = sum_n x[n] sinc(t-n). FFTs evaluate the complete finite
sum on a uniform fractional grid, never a periodic extension. Direct dot/FSUM
evaluation checks/refines extrema. Source files are snapshotted before use.
"""
import argparse
import contextlib
import hashlib
import io
import json
import math
import platform
import shutil
import struct
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent
SOURCE = Path("/tmp/deadpan-master-20260921")
CEILING = 10.0 ** (-1.0 / 20.0)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def load_wave(path):
    data = path.read_bytes()
    fields = struct.unpack("<4sI4s4sIHHIIHH4sI", data[:44])
    assert fields == (
        b"RIFF", len(data) - 8, b"WAVE", b"fmt ", 16, 3, 2, 48000,
        384000, 8, 32, b"data", len(data) - 44,
    ), fields
    pcm = np.frombuffer(data, dtype="<f4", offset=44).reshape(-1, 2).astype(np.float64)
    assert len(pcm) == 8192 and np.isfinite(pcm).all()
    return pcm


class Direct:
    def __init__(self, samples):
        self.samples = samples
        self.indices = np.arange(len(samples), dtype=np.float64)
        signs = np.where(np.arange(len(samples)) % 2 == 0, 1.0, -1.0)
        self.signed = samples * signs

    def value(self, coordinate, compensated=False):
        integer = math.floor(coordinate)
        phase = coordinate - integer
        if phase == 0.0:
            return float(self.samples[integer]) if 0 <= integer < len(self.samples) else 0.0
        # sin(pi(t-n)) = (-1)^(floor(t)-n) sin(pi frac(t)); avoid
        # evaluating trig functions at large arguments for distant samples.
        terms = self.signed / (coordinate - self.indices)
        total = math.fsum(terms) if compensated else float(np.sum(terms))
        factor = math.sin(math.pi * phase) / math.pi
        return (-factor if integer % 2 else factor) * total


def refine(direct, coordinate, radius):
    # Every candidate is a sampled local maximum, refined within its adjacent
    # grid points. This local search is not a separate global-max certificate.
    low, high = coordinate - radius, coordinate + radius
    ratio = (math.sqrt(5.0) - 1.0) / 2.0
    left = high - ratio * (high - low)
    right = low + ratio * (high - low)
    f_left = abs(direct.value(left))
    f_right = abs(direct.value(right))
    for _ in range(56):
        if f_left < f_right:
            low = left
            left, f_left = right, f_right
            right = low + ratio * (high - low)
            f_right = abs(direct.value(right))
        else:
            high = right
            right, f_right = left, f_left
            left = high - ratio * (high - low)
            f_left = abs(direct.value(left))
    candidates = [coordinate, low, high, left, right, (low + high) / 2.0]
    best = max(candidates, key=lambda t: abs(direct.value(t)))
    return best, direct.value(best, compensated=True)


def curvature_bound(sample_count, sample_magnitude):
    # sinc(u) = integral_[−1/2,1/2] exp(2*pi*i*f*u) df gives
    # |sinc''(u)| <= pi^2/3 globally. For |u| >= q >= 1 the differentiated
    # closed form also gives pi/q + 2/q^2 + 2/(pi*q^3).
    # At most two integer samples lie in each distance bucket [q,q+1).
    # These positive bounds decrease with q. Filling the nearest N+1 buckets
    # twice therefore dominates ANY arrangement of the N finite samples,
    # including coordinates outside their support (deliberately conservative).
    q = np.arange(1, sample_count + 1, dtype=np.float64)
    terms = np.minimum(math.pi**2 / 3.0, math.pi / q + 2.0 / q**2 + 2.0 / (math.pi * q**3))
    return 2.0 * sample_magnitude * (math.pi**2 / 3.0 + math.fsum(terms))


def audit_channel(samples, phases, distance, refine_count):
    count = len(samples)
    k_min = -distance
    k_max = count - 1 + distance
    ks = np.arange(k_min, k_max + 1)
    offsets = np.arange(k_min - (count - 1), k_max + 1)
    convolution_length = count + len(offsets) - 1
    fft_length = 1 << (convolution_length - 1).bit_length()
    sample_fft = np.fft.rfft(samples, n=fft_length)
    grid = np.empty((len(ks), phases), dtype=np.float64)
    grid[:, 0] = 0.0
    grid[distance : distance + count, 0] = samples
    signs = np.where(offsets % 2 == 0, 1.0, -1.0)
    for phase_index in range(1, phases):
        phase = phase_index / phases
        # This array includes every lag used by every retained query point.
        # No term of the N-sample sinc sum is dropped or windowed.
        kernel = signs * (math.sin(math.pi * phase) / math.pi) / (offsets + phase)
        convolution = np.fft.irfft(
            sample_fft * np.fft.rfft(kernel, n=fft_length), n=fft_length
        )
        grid[:, phase_index] = convolution[count - 1 : count - 1 + len(ks)]
    values = grid.ravel()
    magnitudes = np.abs(values)
    maximum = int(np.argmax(magnitudes))
    coordinate = k_min + maximum / phases
    direct = Direct(samples)
    grid_value = float(values[maximum])
    direct_grid_value = direct.value(coordinate, compensated=True)
    local_indices = np.flatnonzero(
        (magnitudes[1:-1] >= magnitudes[:-2]) & (magnitudes[1:-1] >= magnitudes[2:])
    ) + 1
    local_indices = np.unique(np.concatenate((local_indices, [0, maximum, len(values) - 1])))
    retain = min(refine_count, len(local_indices))
    selected = local_indices[
        np.argpartition(magnitudes[local_indices], -retain)[-retain:]
    ]
    refined = []
    for index in selected:
        position, value = refine(direct, k_min + int(index) / phases, 1.0 / phases)
        refined.append((abs(value), position, value))
    refined.sort(reverse=True)
    peak, peak_position, peak_value = refined[0]
    rng = np.random.default_rng(942781)
    check_indices = rng.integers(0, len(values), size=64)
    residual = max(
        abs(float(values[index]) - direct.value(k_min + int(index) / phases, compensated=True))
        for index in check_indices
    )
    residual = max(residual, abs(grid_value - direct_grid_value))
    sum_abs = math.fsum(np.abs(samples))
    tail_upper = sum_abs / (math.pi * distance)
    curvature = curvature_bound(count, float(np.max(np.abs(samples))))
    grid_gap_upper = curvature / (8.0 * phases * phases)
    # |f| <= max(abs(endpoint values)) + sup|f''|*h^2/8 on every cell.
    # This is an analytic discretization bound evaluated in ordinary float64,
    # not a directed-rounding proof of FFT/elementary-function arithmetic.
    numerical_allowance = 1e-10 * max(1.0, sum_abs)
    sampled_upper = max(float(magnitudes[maximum]) + grid_gap_upper, tail_upper)
    return {
        "sample_peak": float(np.max(np.abs(samples))),
        "sample_l1": sum_abs,
        "grid_peak": float(magnitudes[maximum]),
        "grid_coordinate": coordinate,
        "direct_at_grid_peak": direct_grid_value,
        "refined_peak": peak,
        "refined_dbtp": 20.0 * math.log10(peak) if peak else None,
        "refined_coordinate": peak_position,
        "refined_signed_value": peak_value,
        "exceeds_minus_1_dbtp": peak > CEILING,
        "top_refined": [
            {"magnitude": magnitude, "coordinate": position, "value": value}
            for magnitude, position, value in refined[:8]
        ],
        "scan_start": k_min,
        "scan_end": k_max + (phases - 1) / phases,
        "outside_distance": distance,
        "outside_tail_upper": tail_upper,
        "sinc_second_derivative_bound": curvature,
        "between_grid_points_allowance": grid_gap_upper,
        "analytic_global_upper_before_numeric_error": sampled_upper,
        "numerical_allowance_not_a_rigorous_error_certificate": numerical_allowance,
        "qualified_numerical_global_upper": sampled_upper + numerical_allowance,
        "fft_direct_max_observed_difference": residual,
        "fft_length": fft_length,
        "fractional_phases": phases,
        "grid_points": len(values),
        "local_maxima": len(local_indices),
        "local_maxima_refined": retain,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phases", type=int, default=128)
    parser.add_argument("--refine", type=int, default=16)
    parser.add_argument("--cases", nargs="*")
    parser.add_argument("--reuse-inputs", action="store_true",
                        help="Reproduce against the retained snapshot without reading the source directory")
    arguments = parser.parse_args()
    assert arguments.phases >= 2 and arguments.refine >= 1
    inputs = ROOT / "inputs"
    inputs.mkdir(exist_ok=True)
    if not arguments.reuse_inputs:
        for name in ["prototype.py", "prototype-kaiser256.json"]:
            shutil.copyfile(SOURCE / name, inputs / name)
    source_report = json.loads((inputs / "prototype-kaiser256.json").read_text())
    source_cases = {item["case"]: item for item in source_report["results"]}
    preferred = ["alternating", "tone-23990-16", "tone-23990-4", "tone-23990-1", "edge-burst"]
    names = arguments.cases or preferred + [name for name in source_cases if name not in preferred]
    if not arguments.reuse_inputs:
        for name in names:
            shutil.copyfile(SOURCE / f"{name}.wav", inputs / f"{name}.wav")
    config = io.StringIO()
    with contextlib.redirect_stdout(config):
        np.show_config()
    metadata = {
        "python": sys.version,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "numpy": np.__version__,
        "numpy_build_config": config.getvalue(),
        "argv": sys.argv,
        "oracle_sha256": sha256(Path(__file__)),
        "source_prototype_sha256": sha256(inputs / "prototype.py"),
        "source_report_sha256": sha256(inputs / "prototype-kaiser256.json"),
        "ceiling_linear": CEILING,
        "interpolation": "complete finite-sequence, zero-extended, unwindowed sinc",
        "arithmetic": "float64 FFT/direct, compensated fsum witnesses; not interval arithmetic",
    }
    write_json(ROOT / "environment.json", metadata)
    start = time.monotonic()
    results = []
    for name in names:
        case_start = time.monotonic()
        path = inputs / f"{name}.wav"
        pcm = load_wave(path)
        assert float(np.max(np.abs(pcm))) == source_cases[name]["sample_peak"], name
        l1 = max(math.fsum(np.abs(pcm[:, channel])) for channel in range(2))
        minimum_distance = max(2, math.ceil(l1 / (math.pi * CEILING)) + 1)
        distance = 1 << (minimum_distance - 1).bit_length()
        channels = [
            audit_channel(pcm[:, channel], arguments.phases, distance, arguments.refine)
            for channel in range(2)
        ]
        result = {
            "case": name,
            "wav_sha256": sha256(path),
            "frames": len(pcm),
            "prototype_same_kernel_dbtp": source_cases[name]["oracle_dbtp"],
            "channels": channels,
            "worst_refined_dbtp": max(channel["refined_dbtp"] for channel in channels),
            "exceeds_minus_1_dbtp": any(channel["exceeds_minus_1_dbtp"] for channel in channels),
            "qualified_numerical_global_upper": max(
                channel["qualified_numerical_global_upper"] for channel in channels
            ),
            "seconds": time.monotonic() - case_start,
        }
        results.append(result)
        write_json(ROOT / "results.json", {"environment": metadata, "results": results,
                   "seconds": time.monotonic() - start})
        print(json.dumps({"case": name, "dbtp": result["worst_refined_dbtp"],
              "exceeds": result["exceeds_minus_1_dbtp"], "seconds": result["seconds"]}), flush=True)


if __name__ == "__main__":
    main()
