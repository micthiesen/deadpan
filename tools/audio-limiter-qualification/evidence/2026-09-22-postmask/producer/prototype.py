"""Bounded scratch experiment for post-mask stereo-linked safety gain.

This is evidence, not production DSP or qualification.  It constructs one
canonical finite project signal, applies an exact silence mask, and proves only
the retained BS.1770 4x and Kaiser-8x/radius-256 finite reconstruction rows.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
import time

import numpy as np


RATE = 48_000
FRAMES = 8_192
K_RADIUS = 256
CONTROL_FIRST = -K_RADIUS
CONTROL_LAST = FRAMES + K_RADIUS - 2  # N + 254
L = 1 / 4_096
R = 1 / 8_192
GAIN_QUANTA = 1 << 32
L_QUANTA = 1 << 20
R_QUANTA = 1 << 19
TARGET_DBTP = -1.25
NUMERIC_GUARD = 1e-6
CEILING = 10 ** (TARGET_DBTP / 20) - NUMERIC_GUARD
PRODUCT_CEILING = 10 ** (-1 / 20)
BOUNDARY_FRAMES = 96

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "source"
OUTPUTS = ROOT / "outputs"
METERS = ROOT / "meters"
COEFF_PATH = SOURCE / "kaiser255-beta16.f64le"
KERNELS_PATH = SOURCE / "combined-kernels.json"
REPO = Path("/Users/michael/Code/deadpan")
METER = REPO / "target/release/examples/measure_pcm"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def write_wave(path: Path, pcm: np.ndarray) -> None:
    data = np.asarray(pcm, dtype="<f4").tobytes()
    header = struct.pack(
        "<4sI4s4sIHHIIHH4sI",
        b"RIFF",
        36 + len(data),
        b"WAVE",
        b"fmt ",
        16,
        3,
        2,
        RATE,
        RATE * 8,
        8,
        32,
        b"data",
        len(data),
    )
    path.write_bytes(header + data)


def kaiser_interpolator() -> tuple[np.ndarray, list[np.ndarray]]:
    offsets = np.arange(1 - K_RADIUS, K_RADIUS + 1, dtype=np.int64)
    kernels: list[np.ndarray] = []
    for phase in range(8):
        distance = phase / 8 - offsets
        window = np.i0(10 * np.sqrt(np.maximum(0, 1 - (distance / K_RADIUS) ** 2))) / np.i0(10)
        weights = np.where(abs(distance) < K_RADIUS, np.sinc(distance) * window, 0.0)
        weights /= np.sum(weights)
        if phase == 0:
            weights[:] = 0
            weights[K_RADIUS - 1] = 1
        kernels.append(weights)
    return offsets, kernels


def boundary_envelope(mask: np.ndarray, mode: str) -> np.ndarray:
    if mode == "hard":
        return np.ones(len(mask), dtype=np.float64)
    if mode != "default96":
        raise ValueError(mode)
    envelope = np.zeros(len(mask), dtype=np.float64)
    at = 0
    while at < len(mask):
        if not mask[at]:
            at += 1
            continue
        end = at + 1
        while end < len(mask) and mask[end]:
            end += 1
        positions = np.arange(end - at)
        envelope[at:end] = np.minimum(
            1.0,
            np.minimum((positions + 1) / BOUNDARY_FRAMES, (end - at - positions) / BOUNDARY_FRAMES),
        )
        at = end
    return envelope


def extended_windows(signal: np.ndarray, offsets: np.ndarray) -> np.ndarray:
    """Return [control anchor, channel, offset] zero-extended windows."""
    first_sample = CONTROL_FIRST + int(offsets[0])
    last_sample = CONTROL_LAST + int(offsets[-1])
    padded = np.zeros((last_sample - first_sample + 1, 2), dtype=np.float64)
    source_first = max(0, first_sample)
    source_last = min(FRAMES - 1, last_sample)
    if source_first <= source_last:
        padded[source_first - first_sample : source_last - first_sample + 1] = signal[source_first : source_last + 1]
    return np.lib.stride_tricks.sliding_window_view(padded, len(offsets), axis=0)


def finite_rows(bs_table: np.ndarray) -> list[tuple[str, np.ndarray, np.ndarray]]:
    rows: list[tuple[str, np.ndarray, np.ndarray]] = [
        ("raw-sample", np.array([0], dtype=np.int64), np.array([1.0])),
    ]
    bs_offsets = np.arange(-6, 6, dtype=np.int64)
    for phase in range(4):
        rows.append((f"bs1770-phase-{phase}", bs_offsets, bs_table[:, phase]))
    k_offsets, k_kernels = kaiser_interpolator()
    for phase in range(1, 8):
        rows.append((f"kaiser8-radius256-phase-{phase}", k_offsets, k_kernels[phase]))
    return rows


def gain_bounds(signal: np.ndarray, rows: list[tuple[str, np.ndarray, np.ndarray]]) -> tuple[np.ndarray, dict]:
    bounds = np.ones(CONTROL_LAST - CONTROL_FIRST + 1, dtype=np.float64)
    diagnostics: dict[str, dict] = {}
    for name, offsets, weights in rows:
        windows = extended_windows(signal, offsets)
        values = np.einsum("ack,k->ac", windows, weights)
        moments = np.einsum("ack,k->ac", abs(windows), abs(weights) * abs(offsets))
        margins = CEILING - L * moments
        margin_min = float(np.min(margins))
        if margin_min <= 0:
            raise ValueError(f"nonpositive finite-proof margin for {name}: {margin_min}")
        allowed = np.divide(margins, abs(values), out=np.ones_like(values), where=values != 0)
        row_bounds = np.minimum(1.0, np.min(allowed, axis=1))
        bounds = np.minimum(bounds, row_bounds)
        diagnostics[name] = {
            "minimum_margin": margin_min,
            "minimum_bound": float(np.min(row_bounds)),
            "maximum_distance_moment_observed": float(np.max(moments)),
        }
    return np.clip(bounds, 0.0, 1.0), diagnostics


def quantized_gain(bounds: np.ndarray) -> tuple[np.ndarray, dict]:
    caps = np.floor(bounds * GAIN_QUANTA).astype(np.uint64)
    caps = np.minimum(caps, GAIN_QUANTA)
    future = caps.copy()
    for index in range(len(future) - 2, -1, -1):
        future[index] = min(future[index], future[index + 1] + L_QUANTA)
    gain = future.copy()
    for index in range(1, len(gain)):
        gain[index] = min(gain[index], gain[index - 1] + R_QUANTA)
    if np.any(gain > caps):
        raise AssertionError("quantized gain exceeds its local cap")
    deltas = np.diff(gain.astype(np.int64))
    if np.min(deltas, initial=0) < -L_QUANTA or np.max(deltas, initial=0) > R_QUANTA:
        raise AssertionError("quantized gain exceeds an exact slope")

    # Same recurrences partitioned into deliberately irregular blocks, carrying
    # their true boundary state.  This checks recurrence partitioning only.
    chunk_sizes = [1, 17, 509, 3, 1024, 97, 4096, 31]
    chunked_future = caps.copy()
    stop = len(caps)
    cursor = 0
    while stop > 0:
        size = chunk_sizes[cursor % len(chunk_sizes)]
        start = max(0, stop - size)
        last = stop - 1 if stop == len(caps) else stop
        for index in range(last - 1, start - 1, -1):
            chunked_future[index] = min(chunked_future[index], chunked_future[index + 1] + L_QUANTA)
        stop = start
        cursor += 1
    chunked_gain = chunked_future.copy()
    start = 0
    cursor = 0
    while start < len(caps):
        stop = min(len(caps), start + chunk_sizes[cursor % len(chunk_sizes)])
        first = max(1, start)
        for index in range(first, stop):
            chunked_gain[index] = min(chunked_gain[index], chunked_gain[index - 1] + R_QUANTA)
        start = stop
        cursor += 1
    if not np.array_equal(future, chunked_future) or not np.array_equal(gain, chunked_gain):
        raise AssertionError("partitioned recurrence differs from one-shot recurrence")
    return gain.astype(np.float64) / GAIN_QUANTA, {
        "cap_quantum": 1 / GAIN_QUANTA,
        "l_step_quanta": L_QUANTA,
        "r_step_quanta": R_QUANTA,
        "partition_check": "bit-identical integer recurrence with carried state",
        "partition_check_scope": "future-cone and release recurrences only; not preparation, crop, or random-seek qualification",
    }


def independent_k8_measure(pcm_f32: np.ndarray) -> dict:
    """Separate post-output finite scan; does not reuse proof values."""
    offsets, kernels = kaiser_interpolator()
    scan = np.asarray(pcm_f32, dtype=np.float64)
    best = {"peak": 0.0, "anchor": None, "phase": None, "channel": None}
    for phase, weights in enumerate(kernels):
        windows = extended_windows(scan, offsets)
        values = np.einsum("ack,k->ac", windows, weights)
        flat = int(np.argmax(abs(values)))
        anchor_index, channel = np.unravel_index(flat, values.shape)
        peak = float(abs(values[anchor_index, channel]))
        if peak > best["peak"]:
            best = {
                "peak": peak,
                "anchor": int(CONTROL_FIRST + anchor_index),
                "phase": phase,
                "channel": int(channel),
            }
    best["dbtp"] = 20 * math.log10(best["peak"]) if best["peak"] else None
    best["passes_minus_1_dbtp"] = best["peak"] <= PRODUCT_CEILING
    best["algorithm"] = "separate post-output 8-phase Kaiser beta10 radius256 finite scan"
    return best


def case_definitions() -> list[dict]:
    n = np.arange(FRAMES)
    cases: list[dict] = []
    alternating_mask = n % 2 == 0
    for amplitude in (0.1, 0.8, 16.0):
        for mode in ("default96", "hard"):
            cases.append(
                {
                    "name": f"mask-alternating-dc-{amplitude:g}-{mode}",
                    "input": np.full((FRAMES, 2), amplitude),
                    "mask": alternating_mask,
                    "mode": mode,
                    "policy": "one-sample active islands alternating with authored silence",
                }
            )
    alternating = (16 * (-1.0) ** n)[:, None]
    cases.append(
        {
            "name": "hot-alternating-16-hard",
            "input": np.repeat(alternating, 2, axis=1),
            "mask": np.ones(FRAMES, dtype=bool),
            "mode": "hard",
            "policy": "unmasked hot alternating control",
        }
    )
    tone = 16 * np.sin(2 * np.pi * 23_990 * (n + 0.5) / RATE)
    cases.append(
        {
            "name": "hot-tone-23990-16-hard",
            "input": np.column_stack((tone, -tone)),
            "mask": np.ones(FRAMES, dtype=bool),
            "mode": "hard",
            "policy": "unmasked near-Nyquist stereo-opposed tone",
        }
    )
    quiet = 0.1 * np.sin(2 * np.pi * 1_000 * (n + 0.5) / RATE)
    cases.append(
        {
            "name": "quiet-tone-1000-0.1-hard",
            "input": np.column_stack((quiet, quiet * 0.25)),
            "mask": np.ones(FRAMES, dtype=bool),
            "mode": "hard",
            "policy": "ordinary quiet unity-gain control",
        }
    )
    cases.append(
        {
            "name": "quiet-dc-0.1-hard",
            "input": np.full((FRAMES, 2), 0.1),
            "mask": np.ones(FRAMES, dtype=bool),
            "mode": "hard",
            "policy": "quiet DC unity-gain control",
        }
    )
    gap_mask = np.ones(FRAMES, dtype=bool)
    gap_mask[3_072:3_584] = False
    gap_tone = 0.8 * np.sin(2 * np.pi * 997 * (n + 0.5) / RATE)
    cases.append(
        {
            "name": "ordinary-silent-gap-default96",
            "input": np.column_stack((gap_tone, -0.75 * gap_tone)),
            "mask": gap_mask,
            "mode": "default96",
            "policy": "authored silent gap; direct signal and tails suppressed exactly",
            "exact_silence_span": [3_072, 3_584],
        }
    )
    tail = np.zeros(FRAMES)
    tail[:3_072] = 0.5 * np.sin(2 * np.pi * 997 * (n[:3_072] + 0.5) / RATE)
    tail[3_072:] = 0.5 * np.exp(-(n[3_072:] - 3_072) / 700) * np.sin(
        2 * np.pi * 997 * (n[3_072:] + 0.5) / RATE
    )
    cases.append(
        {
            "name": "permitted-tail-default96",
            "input": np.column_stack((tail, 0.5 * tail)),
            "mask": np.ones(FRAMES, dtype=bool),
            "mode": "default96",
            "policy": "selected tail remains canonical signal through nominal hold; no silence mask there",
            "tail_span": [3_072, 3_584],
        }
    )
    opposed = np.zeros((FRAMES, 2))
    opposed[4_096] = [16, -16]
    opposed[4_097] = [-16, 16]
    cases.append(
        {
            "name": "stereo-opposed-peaks-hard",
            "input": opposed,
            "mask": np.ones(FRAMES, dtype=bool),
            "mode": "hard",
            "policy": "opposed stereo impulses require one shared gain trajectory",
        }
    )
    return cases


def run_case(case: dict, conditioner: np.ndarray, rows: list[tuple[str, np.ndarray, np.ndarray]]) -> dict:
    started = time.monotonic()
    source = np.asarray(case["input"], dtype=np.float32).astype(np.float64)
    conditioned = np.column_stack(
        [np.convolve(source[:, channel], conditioner, mode="same") for channel in range(2)]
    )
    mask = np.asarray(case["mask"], dtype=bool)
    envelope = boundary_envelope(mask, case["mode"])
    canonical = conditioned * envelope[:, None]
    canonical[~mask] = 0.0
    bounds, row_diagnostics = gain_bounds(canonical, rows)
    extended_gain, recurrence = quantized_gain(bounds)
    gain = extended_gain[-CONTROL_FIRST : -CONTROL_FIRST + FRAMES]
    output64 = canonical * gain[:, None]
    output64[~mask] = 0.0
    output = output64.astype(np.float32)
    if not np.all(np.isfinite(output)):
        raise AssertionError("nonfinite output")
    if np.any(output[~mask] != 0):
        raise AssertionError("masked output is not exact float32 zero")

    stem = case["name"]
    raw_path = OUTPUTS / f"{stem}.f32le"
    wave_path = OUTPUTS / f"{stem}.wav"
    meter_path = METERS / f"{stem}.json"
    output.astype("<f4").tofile(raw_path)
    write_wave(wave_path, output)
    subprocess.run([str(METER), str(raw_path), str(meter_path)], check=True, capture_output=True)
    meter = json.loads(meter_path.read_text())
    official_peak = max(meter["peaks"]["true_peak"])
    independent = independent_k8_measure(output)
    exact_silence = None
    if "exact_silence_span" in case:
        first, last = case["exact_silence_span"]
        exact_silence = bool(np.all(output[first:last] == 0))
    tail_nonzero_frames = None
    if "tail_span" in case:
        first, last = case["tail_span"]
        tail_nonzero_frames = int(np.count_nonzero(np.any(output[first:last] != 0, axis=1)))

    actual_deltas = np.diff(extended_gain)
    result = {
        "case": stem,
        "policy": case["policy"],
        "boundary_mode": case["mode"],
        "input_peak": float(np.max(abs(source))),
        "conditioned_peak": float(np.max(abs(conditioned))),
        "canonical_peak_before_gain": float(np.max(abs(canonical))),
        "sample_peak": float(np.max(abs(output))),
        "gain_min": float(np.min(gain)),
        "gain_max": float(np.max(gain)),
        "gain_all_unity": bool(np.all(gain == 1.0)),
        "shared_gain_channels": True,
        "maximum_gain_down_step": float(max(0.0, -np.min(actual_deltas, initial=0.0))),
        "maximum_gain_up_step": float(max(0.0, np.max(actual_deltas, initial=0.0))),
        "minimum_boundary_envelope_on_active": float(np.min(envelope[mask])) if np.any(mask) else None,
        "active_output_frames": int(np.count_nonzero(mask)),
        "exact_silence_span_is_zero": exact_silence,
        "permitted_tail_nonzero_frames": tail_nonzero_frames,
        "official_bs1770_meter": {
            "peak": official_peak,
            "dbtp": 20 * math.log10(official_peak) if official_peak else None,
            "passes_minus_1_dbtp": official_peak <= PRODUCT_CEILING,
            "report": str(meter_path),
        },
        "independent_finite_detector": independent,
        "finite_proof_rows": row_diagnostics,
        "recurrence": recurrence,
        "artifacts": {
            "wav": str(wave_path),
            "wav_sha256": sha256(wave_path),
            "f32le": str(raw_path),
            "f32le_sha256": sha256(raw_path),
            "meter": str(meter_path),
            "meter_sha256": sha256(meter_path),
        },
        "elapsed_seconds": time.monotonic() - started,
    }
    result["passes_both_retained_finite_detectors"] = (
        result["official_bs1770_meter"]["passes_minus_1_dbtp"]
        and result["independent_finite_detector"]["passes_minus_1_dbtp"]
    )
    return result


def main() -> None:
    OUTPUTS.mkdir(exist_ok=True)
    METERS.mkdir(exist_ok=True)
    conditioner = np.fromfile(COEFF_PATH, dtype="<f8")
    if len(conditioner) != 255:
        raise ValueError("expected 255 conditioner taps")
    kernels_json = json.loads(KERNELS_PATH.read_text())
    bs_table = np.array(kernels_json["bs1770_table_rows_oldest_to_newest"], dtype=np.float64)
    if bs_table.shape != (12, 4):
        raise ValueError(f"unexpected BS.1770 table shape {bs_table.shape}")
    rows = finite_rows(bs_table)

    pre_run = {
        "schema_version": 1,
        "captured_before_case_outputs": True,
        "utc_epoch_seconds": time.time(),
        "python": sys.version,
        "platform": platform.platform(),
        "numpy": np.__version__,
        "hashes": {
            "prototype.py": sha256(Path(__file__)),
            "kaiser255-beta16.f64le": sha256(COEFF_PATH),
            "combined-kernels.json": sha256(KERNELS_PATH),
            "measure_pcm": sha256(METER),
        },
        "paths": {
            "prototype.py": str(Path(__file__)),
            "kaiser255-beta16.f64le": str(COEFF_PATH),
            "combined-kernels.json": str(KERNELS_PATH),
            "measure_pcm": str(METER),
        },
    }
    (ROOT / "pre-run-manifest.json").write_text(json.dumps(pre_run, indent=2) + "\n")

    conditioner_l1 = float(np.sum(abs(conditioner)))
    _, k_kernels = kaiser_interpolator()
    k_max_distance = max(
        float(np.sum(abs(kernel) * abs(np.arange(1 - K_RADIUS, K_RADIUS + 1))))
        for kernel in k_kernels
    )
    report = {
        "schema_version": 1,
        "status": "running",
        "claim": "bounded scratch evidence only; not production qualification and not an ideal-sinc ceiling proof",
        "signal_order": [
            "255-tap beta16 conditioner",
            "explicit default96 boundary envelope or hard mode",
            "exact authored silence mask",
            "shared stereo-linked quantized finite-row safety gain",
            "float32 output",
        ],
        "configuration": {
            "rate_hz": RATE,
            "frames": FRAMES,
            "conditioner_taps": len(conditioner),
            "conditioner_l1": conditioner_l1,
            "conditioned_magnitude_cap_for_abs_input_le_16": 16 * conditioner_l1,
            "target_dbtp": TARGET_DBTP,
            "numeric_guard_linear": NUMERIC_GUARD,
            "effective_internal_ceiling_linear": CEILING,
            "L": L,
            "R": R,
            "gain_quantum": 1 / GAIN_QUANTA,
            "control_anchor_domain_inclusive": [CONTROL_FIRST, CONTROL_LAST],
            "k8_offsets_inclusive": [1 - K_RADIUS, K_RADIUS],
            "bs1770_offsets_inclusive": [-6, 5],
            "lookahead_samples": 4_096 + 256 + 127,
            "lookahead_ms": (4_096 + 256 + 127) * 1_000 / RATE,
            "default_boundary_frames": BOUNDARY_FRAMES,
            "default_one_sample_fragment_factor": 1 / BOUNDARY_FRAMES,
            "default_one_sample_fragments_erased": False,
        },
        "worst_case_cap_context": {
            "arbitrary_input_absolute_bound": 16,
            "conditioned_absolute_bound": 16 * conditioner_l1,
            "k8_max_integer_origin_distance_moment": k_max_distance,
            "Wmax_product": 16 * conditioner_l1 * k_max_distance,
            "minimum_margin_from_global_caps": CEILING - L * 16 * conditioner_l1 * k_max_distance,
            "scope": "finite K8/radius256 and smaller retained rows; mask/envelope cannot increase canonical sample magnitude",
        },
        "pre_run_manifest": str(ROOT / "pre-run-manifest.json"),
        "results": [],
        "failures": [],
    }
    results_path = ROOT / "results.json"
    results_path.write_text(json.dumps(report, indent=2) + "\n")
    started = time.monotonic()
    for case in case_definitions():
        try:
            result = run_case(case, conditioner, rows)
            report["results"].append(result)
            print(
                result["case"],
                f"BS={result['official_bs1770_meter']['dbtp']:.6f}",
                f"K8={result['independent_finite_detector']['dbtp']:.6f}",
                f"gain={result['gain_min']:.9f}..{result['gain_max']:.9f}",
                flush=True,
            )
        except Exception as error:  # retain failures and continue bounded corpus
            report["failures"].append({"case": case["name"], "type": type(error).__name__, "message": str(error)})
            print(case["name"], "FAILED", repr(error), flush=True)
        results_path.write_text(json.dumps(report, indent=2) + "\n")
    report["status"] = "complete_with_failures" if report["failures"] else "complete"
    report["elapsed_seconds"] = time.monotonic() - started
    report["all_cases_pass_both_retained_finite_detectors"] = bool(report["results"]) and all(
        result["passes_both_retained_finite_detectors"] for result in report["results"]
    ) and not report["failures"]
    report["all_default_alternating_one_sample_fragments_nonzero"] = all(
        result["minimum_boundary_envelope_on_active"] == 1 / BOUNDARY_FRAMES
        for result in report["results"]
        if result["case"].startswith("mask-alternating") and result["boundary_mode"] == "default96"
    )
    results_path.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
