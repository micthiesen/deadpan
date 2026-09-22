"""Scratch-only 48 kHz Type-I output-conditioner design and measurement.

SciPy's BSD-licensed firwin implementation generates coefficients. No limiter,
mask, gain envelope, PCM renderer, or product code is implemented here.
"""
import contextlib
import hashlib
import io
import json
import math
import platform
import sys
import time
from fractions import Fraction
from pathlib import Path

import numpy as np
import scipy
from scipy import optimize, signal

ROOT = Path(__file__).resolve().parent
RATE = 48_000
GRID_SIZE = 1_048_577
Q_BITS = 52


def write_json(name, value):
    (ROOT / name).write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def db(value):
    return 20.0 * math.log10(value) if value > 0 else None


def coefficients(raw):
    # Equal integer parity sums make both identities exact for the stored binary
    # coefficients: H(0)=sum h=1 and H(pi)=sum (-1)^n h=0.
    symmetric = (raw + raw[::-1]) / 2.0
    scale = 1 << Q_BITS
    integers = [round(float(value) * scale) for value in symmetric]
    assert integers == integers[::-1]
    center = len(integers) // 2
    other_parity = (center - 1) % 2
    correction = scale // 2 - sum(integers[other_parity::2])
    assert correction % 2 == 0
    integers[center - 1] += correction // 2
    integers[center + 1] += correction // 2
    integers[center] += scale // 2 - sum(integers[center % 2::2])
    assert integers == integers[::-1]
    assert sum(integers[0::2]) == sum(integers[1::2]) == scale // 2
    assert max(abs(value) for value in integers) < 1 << 53
    stored = np.array([value / scale for value in integers], dtype=np.float64)
    fractions = [Fraction(float(value)) for value in stored]
    assert sum(fractions) == 1
    assert sum(value * (-1 if index % 2 else 1) for index, value in enumerate(fractions)) == 0
    assert np.array_equal(stored, stored[::-1])
    return stored, integers, {
        "quantization_fractional_bits": Q_BITS,
        "parity_sums_exact": "even=1/2, odd=1/2",
        "dc_exact_rational": "1",
        "nyquist_exact_rational": "0",
        "dc_math_fsum": math.fsum(stored),
        "nyquist_math_fsum": math.fsum(value * (-1 if i % 2 else 1) for i, value in enumerate(stored)),
        "dc_numpy_sum": float(np.sum(stored)),
        "nyquist_numpy_sum": float(np.sum(stored * (-1.0) ** np.arange(len(stored)))),
        "max_coefficient_change_from_design": float(np.max(np.abs(stored - raw))),
        "l1_coefficient_change_from_design": math.fsum(np.abs(stored - raw)),
    }


class CenteredResponse:
    def __init__(self, taps):
        self.center = len(taps) // 2
        self.middle = float(taps[self.center])
        self.terms = 2.0 * taps[self.center + 1:]
        self.indices = np.arange(1, self.center + 1, dtype=np.float64)

    def value(self, frequency):
        omega = math.tau * frequency / RATE
        return self.middle + math.fsum(self.terms * np.cos(self.indices * omega))

    def derivative(self, frequency):
        omega = math.tau * frequency / RATE
        return -math.fsum(self.terms * self.indices * np.sin(self.indices * omega))

    def second_derivative_bound(self):
        return math.fsum(np.abs(self.terms) * self.indices**2)


def measure(taps):
    center = len(taps) // 2
    response = CenteredResponse(taps)
    frequencies, transfer = signal.freqz(taps, worN=GRID_SIZE, fs=RATE, include_nyquist=True)
    centered = np.real(transfer * np.exp(1j * math.tau * frequencies * center / RATE))

    # Isolate sign changes of the analytic derivative on a separate grid, then
    # refine every bracket. Dense-grid bounds below do not depend on finding all
    # stationary points or on assuming one maximum per bracket.
    coarse, derivative_transfer = signal.freqz(
        taps * (center - np.arange(len(taps))), worN=32_769,
        fs=RATE, include_nyquist=True,
    )
    derivative = -np.imag(derivative_transfer * np.exp(1j * math.tau * coarse * center / RATE))
    roots = []
    for index in np.flatnonzero(derivative[:-1] * derivative[1:] < 0):
        low, high = float(coarse[index]), float(coarse[index + 1])
        # At extremely small derivatives FFT roundoff can manufacture a sign
        # change; recheck the direct cosine-series derivative before Brent.
        if response.derivative(low) * response.derivative(high) >= 0:
            continue
        roots.append(float(optimize.brentq(response.derivative, low, high, xtol=1e-9, rtol=1e-14)))

    candidates = sorted(set([0.0, 20_000.0, 22_000.0, 24_000.0] + roots))
    evaluated = [(frequency, response.value(frequency)) for frequency in candidates]
    passing = [(frequency, value) for frequency, value in evaluated if frequency <= 20_000]
    stopping = [(frequency, abs(value)) for frequency, value in evaluated if frequency >= 22_000]
    pass_min = min(passing, key=lambda item: item[1])
    pass_max = max(passing, key=lambda item: item[1])
    stop_max = max(stopping, key=lambda item: item[1])
    assert pass_min[1] > 0
    pass_grid = centered[frequencies <= 20_000]
    stop_grid = np.abs(centered[frequencies >= 22_000])
    grid_min = min(float(np.min(pass_grid)), response.value(20_000.0))
    grid_max = max(float(np.max(pass_grid)), response.value(20_000.0))
    grid_stop = max(float(np.max(stop_grid)), abs(response.value(22_000.0)))
    omega_step = math.pi / (GRID_SIZE - 1)
    curvature = response.second_derivative_bound()
    grid_allowance = curvature * omega_step**2 / 8.0
    numerical_allowance = 1e-12 * max(1.0, math.fsum(np.abs(taps)))
    allowance = grid_allowance + numerical_allowance
    pass_lower = grid_min - allowance
    pass_upper = grid_max + allowance
    stop_upper = grid_stop + allowance
    pass_ripple_bound = 20.0 * math.log10(pass_upper / pass_lower)
    maximum_pass_deviation_bound = max(abs(db(pass_lower)), abs(db(pass_upper)))
    stop_db_bound = db(stop_upper)

    # Independent consistency checks for the stored f64 coefficients.
    rng = np.random.default_rng(27571)
    check_indices = rng.integers(0, GRID_SIZE, size=128)
    fft_direct_error = max(
        abs(float(centered[index]) - response.value(float(frequencies[index])))
        for index in check_indices
    )
    assert fft_direct_error < numerical_allowance
    assert pass_min[1] >= pass_lower and pass_max[1] <= pass_upper
    assert stop_max[1] <= stop_upper
    impulse = np.convolve(np.array([1.0]), taps)
    assert np.array_equal(impulse, taps)
    delay_frequencies, delays = signal.group_delay((taps, [1.0]), w=[0, 1000, 10000, 20000], fs=RATE)
    delay_error = float(np.max(np.abs(delays - center)))
    assert delay_error < 1e-9
    return {
        "frames": len(taps), "type": "I", "rate_hz": RATE,
        "symmetry_bit_exact": True,
        "group_delay_frames_exact": center,
        "group_delay_milliseconds": center * 1000.0 / RATE,
        "group_delay_measured_max_error_frames": delay_error,
        "group_delay_check_frequencies_hz": delay_frequencies.tolist(),
        "impulse_equals_coefficients": True,
        "passband_min": {"hz": pass_min[0], "gain": pass_min[1], "db": db(pass_min[1])},
        "passband_max": {"hz": pass_max[0], "gain": pass_max[1], "db": db(pass_max[1])},
        "passband_refined_peak_to_peak_db": db(pass_max[1] / pass_min[1]),
        "stopband_max": {"hz": stop_max[0], "gain": stop_max[1], "db": db(stop_max[1])},
        "refined_stationary_points": len(roots),
        "dense_grid_points": GRID_SIZE,
        "dense_grid_spacing_hz": RATE / (2.0 * (GRID_SIZE - 1)),
        "response_second_derivative_bound_per_radian_squared": curvature,
        "between_grid_allowance": grid_allowance,
        "numerical_allowance_not_interval_certified": numerical_allowance,
        "fft_direct_max_observed_error": fft_direct_error,
        "qualified_numerical_passband_ripple_bound_db": pass_ripple_bound,
        "qualified_numerical_passband_max_deviation_bound_db": maximum_pass_deviation_bound,
        "qualified_numerical_stopband_upper_db": stop_db_bound,
        "meets_fixed_filter_targets": pass_ripple_bound <= 0.005
            and maximum_pass_deviation_bound <= 0.005 and stop_db_bound <= -120.0,
        "selected_frequency_response_db": {
            str(frequency): db(abs(response.value(frequency)))
            for frequency in [0, 1000, 10000, 20000, 20500, 21000, 21500, 22000, 23000, 23990, 24000]
        },
    }


def main():
    start = time.monotonic()
    config = io.StringIO()
    with contextlib.redirect_stdout(config):
        scipy.show_config()
    environment = {
        "python": sys.version, "platform": platform.platform(),
        "numpy": np.__version__, "scipy": scipy.__version__,
        "scipy_build_configuration": config.getvalue(),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "design_source": "SciPy firwin, BSD-3-Clause; no copied GPL code",
        "primary_sources": [
            "https://docs.scipy.org/doc/scipy-1.18.0/reference/generated/scipy.signal.firwin.html",
            "https://docs.scipy.org/doc/scipy-1.18.0/reference/generated/scipy.signal.remez.html",
            "https://github.com/scipy/scipy/blob/v1.18.0/LICENSE.txt",
        ],
    }
    write_json("environment.json", environment)
    attempts = []
    for count in [255, 511]:
        for weight in [1, 30, 300]:
            try:
                signal.remez(count, [0, 20000, 22000, 24000], [1, 0],
                             weight=[1, weight], fs=RATE, grid_density=32, maxiter=100)
                attempts.append({"taps": count, "stopband_weight": weight, "outcome": "returned coefficients; not selected"})
            except ValueError as error:
                attempts.append({"taps": count, "stopband_weight": weight, "outcome": str(error).strip()})
    write_json("remez-attempts.json", attempts)
    results = []
    for count in [255, 511]:
        for beta in [12, 14, 16, 18]:
            name = f"kaiser{count}-beta{beta}"
            raw = signal.firwin(count, 21000, window=("kaiser", beta), scale=True, fs=RATE)
            stored, integers, quantization = coefficients(raw)
            stored_bytes = stored.astype("<f8").tobytes()
            (ROOT / (name + ".f64le")).write_bytes(stored_bytes)
            write_json(name + ".json", {
                "name": name, "rate_hz": RATE, "half_amplitude_cutoff_hz": 21000,
                "window": "symmetric Kaiser", "beta": beta,
                "coefficients_f64": stored.tolist(), "integer_coefficients": integers,
                "integer_denominator": 1 << Q_BITS,
                "raw_f64le_sha256": hashlib.sha256(stored_bytes).hexdigest(),
                "quantization": quantization,
            })
            result = {"name": name, "quantization": quantization,
                      "measurement": measure(stored)}
            results.append(result)
            write_json("results.json", {"environment": environment, "results": results,
                       "remez_attempts": attempts, "seconds": time.monotonic() - start})
            m = result["measurement"]
            print(json.dumps({"name": name, "passband_ripple_db": m["passband_refined_peak_to_peak_db"],
                  "stopband_max_db": m["stopband_max"]["db"],
                  "stopband_bound_db": m["qualified_numerical_stopband_upper_db"],
                  "meets": m["meets_fixed_filter_targets"]}), flush=True)


if __name__ == "__main__":
    main()
