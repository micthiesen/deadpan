#!/usr/bin/env python3
"""Independent PCM analysis for the canonical audio adapter probe."""
from __future__ import annotations

from array import array
import importlib.util
import math
from pathlib import Path


_BASE_SPEC = importlib.util.spec_from_file_location(
    "deadpan_audio_baseline_analysis", Path(__file__).with_name("run.py")
)
if _BASE_SPEC is None or _BASE_SPEC.loader is None:
    raise RuntimeError("could not load the baseline audio analysis helpers")
_BASE = importlib.util.module_from_spec(_BASE_SPEC)
_BASE_SPEC.loader.exec_module(_BASE)

RATE = 48_000
CHANNELS = 2
QUANTUM = 256
INVALID_REQUESTS_REJECTED = 12
WINDOWS = ("120-30", "120-15", "60-15")
CONSUMERS = ("preview", "irregular", "export")
ROOT_KEYS = {
    "sample_rate",
    "channels",
    "quantum",
    "analysis_window_ms",
    "invalid_requests_rejected",
    "cases",
}
CASE_KEYS = {
    "name",
    "fixture_kind",
    "input_file",
    "input_frames",
    "output_frames",
    "pitch_semitones",
    "renders",
    "seeks",
}
CONSUMER_STATS_KEYS = {
    "calls",
    "new_calls",
    "new_bytes",
    "milliseconds",
    "maximum_call_ms",
    "dsp_calls",
    "maximum_buffer_frames",
    "leading_output_frames",
    "lookahead_input_frames",
    "configure",
}
CONFIGURE_STATS_KEYS = {
    "calls",
    "new_calls",
    "new_bytes",
    "milliseconds",
    "maximum_call_ms",
}
TARGETS = {
    **_BASE.TARGETS,
    "canonical_normalized_rms_max": 0.0,
    "maximum_buffer_frames_max": 256,
    "steady_cpp_new_bytes_max": 0,
}


def _nearest_even_ratio(value: int, numerator: int, denominator: int) -> int:
    whole, remainder = divmod(value * numerator, denominator)
    if remainder * 2 > denominator or (remainder * 2 == denominator and whole % 2):
        whole += 1
    return whole


def _expected_cases() -> dict[str, dict]:
    expected = {}
    speeds = ((1, 2), (3, 4), (1, 1), (3, 2), (2, 1))
    for numerator, denominator in speeds:
        for pitch in (-7, 0, 7):
            name = f"mixed-{numerator}-{denominator}-pitch-{pitch}"
            expected[name] = {
                "fixture_kind": "mixed",
                "input_file": "input.f32",
                "input_frames": 192_192,
                "output_frames": _nearest_even_ratio(192_192, denominator, numerator),
                "pitch_semitones": pitch,
            }
    for input_frames in (31, 1003):
        for numerator, denominator in speeds:
            for pitch in (-7, 0, 7):
                name = f"short-{input_frames}-{numerator}-{denominator}-pitch-{pitch}"
                expected[name] = {
                    "fixture_kind": "impulse",
                    "input_file": f"short-{input_frames}-input.f32",
                    "input_frames": input_frames,
                    "output_frames": _nearest_even_ratio(
                        input_frames, denominator, numerator
                    ),
                    "pitch_semitones": pitch,
                }
    for input_frames in (1, 2, 5759, 5760, 5761):
        name = f"edge-{input_frames}"
        expected[name] = {
            "fixture_kind": "impulse",
            "input_file": f"{name}-input.f32",
            "input_frames": input_frames,
            "output_frames": input_frames,
            "pitch_semitones": 0,
        }
    if len(expected) != 50:
        raise RuntimeError("internal canonical case manifest is incomplete")
    return expected


EXPECTED_CASES = _expected_cases()


def _require_mapping(value, path: str) -> dict:
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected object")
    return value


def _require_exact_keys(value, expected: set[str], path: str) -> dict:
    mapping = _require_mapping(value, path)
    actual = set(mapping)
    if actual != expected:
        missing = sorted(expected - actual)
        unexpected = sorted((repr(key) for key in actual - expected))
        raise ValueError(f"{path}: manifest keys differ; missing={missing}, unexpected={unexpected}")
    return mapping


def _require_nonnegative_int(value, path: str) -> None:
    if type(value) is not int or value < 0:
        raise ValueError(f"{path}: expected nonnegative integer")


def _require_nonnegative_number(value, path: str) -> None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{path}: expected nonnegative finite number")
    if not math.isfinite(value) or value < 0:
        raise ValueError(f"{path}: expected nonnegative finite number")


def _validate_stats(value, path: str) -> None:
    stats = _require_exact_keys(value, CONSUMER_STATS_KEYS, path)
    for key in (
        "calls",
        "new_calls",
        "new_bytes",
        "dsp_calls",
        "maximum_buffer_frames",
        "leading_output_frames",
        "lookahead_input_frames",
    ):
        _require_nonnegative_int(stats[key], f"{path}.{key}")
    for key in ("milliseconds", "maximum_call_ms"):
        _require_nonnegative_number(stats[key], f"{path}.{key}")
    configure = _require_exact_keys(
        stats["configure"], CONFIGURE_STATS_KEYS, f"{path}.configure"
    )
    for key in ("calls", "new_calls", "new_bytes"):
        _require_nonnegative_int(configure[key], f"{path}.configure.{key}")
    for key in ("milliseconds", "maximum_call_ms"):
        _require_nonnegative_number(configure[key], f"{path}.configure.{key}")


def _expected_seek_windows(output_frames: int) -> list[tuple[int, int]]:
    starts = (
        output_frames * 4 // 5,
        output_frames // 7,
        output_frames - 1,
        output_frames // 2,
    )
    return [(start, min(4800, output_frames - start)) for start in starts]


def _validate_manifest(native, expected_window: str) -> None:
    """Reject incomplete or altered production probe output before PCM analysis."""

    if expected_window not in WINDOWS:
        raise ValueError(f"expected_window: unsupported value {expected_window!r}")
    root = _require_exact_keys(native, ROOT_KEYS, "native")
    expected_root = {
        "sample_rate": RATE,
        "channels": CHANNELS,
        "quantum": QUANTUM,
        "analysis_window_ms": expected_window,
        "invalid_requests_rejected": INVALID_REQUESTS_REJECTED,
    }
    for key, expected in expected_root.items():
        if root[key] != expected or type(root[key]) is not type(expected):
            raise ValueError(
                f"native.{key}: expected {expected!r}, received {root[key]!r}"
            )
    cases = root["cases"]
    if not isinstance(cases, list):
        raise ValueError("native.cases: expected array")
    if len(cases) != len(EXPECTED_CASES):
        raise ValueError(
            f"native.cases: expected {len(EXPECTED_CASES)} cases, received {len(cases)}"
        )
    names = []
    for index, value in enumerate(cases):
        case = _require_exact_keys(value, CASE_KEYS, f"native.cases[{index}]")
        name = case["name"]
        if not isinstance(name, str):
            raise ValueError(f"native.cases[{index}].name: expected string")
        names.append(name)
    if len(set(names)) != len(names):
        raise ValueError("native.cases: duplicate case name")
    actual_names = set(names)
    expected_names = set(EXPECTED_CASES)
    if actual_names != expected_names:
        raise ValueError(
            "native.cases: case names differ; "
            f"missing={sorted(expected_names - actual_names)}, "
            f"unexpected={sorted(actual_names - expected_names)}"
        )

    for index, case in enumerate(cases):
        name = case["name"]
        expected = EXPECTED_CASES[name]
        for key, expected_value in expected.items():
            if case[key] != expected_value or type(case[key]) is not type(expected_value):
                raise ValueError(
                    f"native.cases[{index}].{key}: expected {expected_value!r}, "
                    f"received {case[key]!r}"
                )
        renders = _require_mapping(case["renders"], f"native.cases[{index}].renders")
        if set(renders) != set(CONSUMERS):
            raise ValueError(
                f"native.cases[{index}].renders: expected exactly {list(CONSUMERS)}"
            )
        for consumer in CONSUMERS:
            _validate_stats(
                renders[consumer], f"native.cases[{index}].renders.{consumer}"
            )

        seeks = case["seeks"]
        if not isinstance(seeks, list) or len(seeks) != 4:
            raise ValueError(f"native.cases[{index}].seeks: expected four seek records")
        for seek_index, ((expected_start, expected_frames), value) in enumerate(
            zip(_expected_seek_windows(case["output_frames"]), seeks)
        ):
            path = f"native.cases[{index}].seeks[{seek_index}]"
            seek = _require_exact_keys(
                value, {"start_frame", "frames", "replay", "cached"}, path
            )
            if type(seek["start_frame"]) is not int or seek["start_frame"] != expected_start:
                raise ValueError(f"{path}.start_frame: expected {expected_start}")
            if type(seek["frames"]) is not int or seek["frames"] != expected_frames:
                raise ValueError(f"{path}.frames: expected {expected_frames}")
            _validate_stats(seek["replay"], f"{path}.replay")
            _validate_stats(seek["cached"], f"{path}.cached")


def _difference(reference: array, actual: array) -> dict:
    if len(reference) != len(actual):
        return {
            "bit_exact": False,
            "peak_error": None,
            "normalized_rms_error": None,
            "reference_samples": len(reference),
            "actual_samples": len(actual),
        }
    deltas = [left - right for left, right in zip(reference, actual)]
    reference_rms = _BASE.rms(reference)
    return {
        "bit_exact": reference.tobytes() == actual.tobytes(),
        "peak_error": max((abs(value) for value in deltas), default=0.0),
        "normalized_rms_error": _BASE.rms(deltas) / max(reference_rms, 1e-15),
        "reference_samples": len(reference),
        "actual_samples": len(actual),
    }


def _compare(
    assertions: list[dict], category: str, case_name: str, reference: array, actual: array
) -> dict:
    difference = _difference(reference, actual)
    _BASE.check(
        assertions,
        category,
        case_name,
        difference["bit_exact"],
        difference,
        {"bit_exact": True, "normalized_rms_error_max": TARGETS["canonical_normalized_rms_max"]},
    )
    return difference


def _check_stats(assertions: list[dict], case_name: str, label: str, stats: dict) -> None:
    new_calls = stats["new_calls"]
    new_bytes = stats["new_bytes"]
    maximum_buffer_frames = stats["maximum_buffer_frames"]
    _BASE.check(
        assertions,
        "steady_cpp_new_calls",
        f"{case_name}/{label}",
        new_calls <= TARGETS["steady_cpp_new_calls_max"],
        new_calls,
        TARGETS["steady_cpp_new_calls_max"],
    )
    _BASE.check(
        assertions,
        "steady_cpp_new_bytes",
        f"{case_name}/{label}",
        new_bytes <= TARGETS["steady_cpp_new_bytes_max"],
        new_bytes,
        TARGETS["steady_cpp_new_bytes_max"],
    )
    _BASE.check(
        assertions,
        "bounded_audio_buffer",
        f"{case_name}/{label}",
        maximum_buffer_frames <= TARGETS["maximum_buffer_frames_max"],
        maximum_buffer_frames,
        TARGETS["maximum_buffer_frames_max"],
    )


def _mixed_measurements(
    case: dict, original: array, output: array, assertions: list[dict]
) -> dict:
    name = case["name"]
    input_frames = case["input_frames"]
    output_frames = case["output_frames"]
    speed = input_frames / output_frames
    pitch_ratio = 2 ** (case["pitch_semitones"] / 12)
    values = {
        "fixture_kind": "mixed",
        "actual_rate": {
            "input_frames": input_frames,
            "output_frames": output_frames,
            "speed": speed,
        },
        "tones": [],
        "channel_ratios": [],
        "dynamics": [],
        "chirps": [],
        "transients": [],
    }
    _BASE.check(
        assertions,
        "mixed_fixture_duration",
        name,
        input_frames == 192_192,
        input_frames,
        192_192,
    )
    for channel, base_hz in enumerate((440, 660)):
        loud = _BASE.window(output, channel, 0.45, 0.8, speed)
        quiet = _BASE.window(output, channel, 3.40, 3.60, speed)
        actual_hz = _BASE.frequency(loud)
        expected_hz = base_hz * pitch_ratio
        cents = 1200 * math.log2(actual_hz / expected_hz)
        values["tones"].append(
            {
                "channel": channel,
                "expected_hz": expected_hz,
                "measured_hz": actual_hz,
                "error_cents": cents,
            }
        )
        _BASE.check(
            assertions,
            "independent_pitch",
            f"{name}/channel-{channel}",
            abs(cents) <= TARGETS["tone_pitch_cents_max"],
            cents,
            TARGETS["tone_pitch_cents_max"],
        )

        dynamic_db = _BASE.decibels_ratio(_BASE.rms(quiet), _BASE.rms(loud))
        values["dynamics"].append(
            {"channel": channel, "quiet_to_loud_db": dynamic_db, "expected_db": -20}
        )
        _BASE.check(
            assertions,
            "preserve_authored_dynamics",
            f"{name}/channel-{channel}",
            abs(dynamic_db + 20) <= TARGETS["dynamic_ratio_error_db_max"],
            dynamic_db,
            {"expected_db": -20, "tolerance_db": TARGETS["dynamic_ratio_error_db_max"]},
        )

        chirp_hz = _BASE.frequency(_BASE.window(output, channel, 1.58, 1.62, speed))
        values["chirps"].append(
            {
                "channel": channel,
                "mean_zero_crossing_hz": chirp_hz,
                "authored_center_hz_times_pitch": (1040 if channel == 0 else 620)
                * pitch_ratio,
            }
        )

        expected = round((2.25 if channel == 0 else 2.3) * RATE / speed)
        first = max(0, expected - round(0.15 * RATE / speed))
        last = min(output_frames, expected + round(0.15 * RATE / speed))
        local = list(output[2 * first + channel : 2 * last : 2])
        peak = max(range(len(local)), key=lambda index: abs(local[index]))
        energy = math.fsum(sample * sample for sample in local)
        cumulative = 0.0
        lower = upper = None
        for index, sample in enumerate(local):
            cumulative += sample * sample
            if lower is None and cumulative >= 0.05 * energy:
                lower = index
            if upper is None and cumulative >= 0.95 * energy:
                upper = index
        error = first + peak - expected
        values["transients"].append(
            {
                "channel": channel,
                "peak_error_samples": error,
                "peak_amplitude": local[peak],
                "energy_90_percent_width_samples": upper - lower,
            }
        )
        _BASE.check(
            assertions,
            "transient_position",
            f"{name}/channel-{channel}",
            abs(error) <= TARGETS["transient_peak_error_samples_max"],
            error,
            TARGETS["transient_peak_error_samples_max"],
        )

    for start, end, ratio in ((0.45, 0.8, 0.25), (2.65, 2.85, 4.0)):
        expected_db = 20 * math.log10(ratio)
        actual_db = _BASE.decibels_ratio(
            _BASE.rms(_BASE.window(output, 1, start, end, speed)),
            _BASE.rms(_BASE.window(output, 0, start, end, speed)),
        )
        values["channel_ratios"].append(
            {
                "source_window_seconds": [start, end],
                "expected_db": expected_db,
                "measured_db": actual_db,
            }
        )
        _BASE.check(
            assertions,
            "preserve_channel_levels",
            f"{name}/{start}",
            abs(actual_db - expected_db) <= TARGETS["channel_ratio_error_db_max"],
            actual_db,
            {"expected_db": expected_db, "tolerance_db": TARGETS["channel_ratio_error_db_max"]},
        )

    values["embedded_silence_rms"] = [
        _BASE.rms(_BASE.window(output, channel, 1.10, 1.15, speed)) for channel in range(CHANNELS)
    ]
    if input_frames == output_frames and case["pitch_semitones"] == 0:
        identity_error = _BASE.normalized_difference(original, output)
        values["unity_identity_normalized_rms_error"] = identity_error
        _BASE.check(
            assertions,
            "unity_identity",
            name,
            identity_error <= TARGETS["identity_normalized_rms_max"],
            identity_error,
            TARGETS["identity_normalized_rms_max"],
        )
    return values


def _channel_energy_and_peak(samples: array, channel: int) -> tuple[float, int, float]:
    values = samples[channel::CHANNELS]
    energy = math.fsum(sample * sample for sample in values)
    peak_frame = max(range(len(values)), key=lambda index: abs(values[index]))
    return energy, peak_frame, values[peak_frame]


def _impulse_measurements(
    case: dict, original: array, outputs: dict[str, array], assertions: list[dict]
) -> dict:
    name = case["name"]
    authored_peak = case["input_frames"] // 3
    expected_values = (array("f", [0.8])[0], array("f", [-0.2])[0])
    input_channels = []
    for channel, expected_value in enumerate(expected_values):
        values = original[channel::CHANNELS]
        nonzero = [index for index, value in enumerate(values) if value != 0]
        fixture_ok = len(nonzero) == 1 and nonzero[0] == authored_peak and values[authored_peak] == expected_value
        diagnostic = {
            "channel": channel,
            "nonzero_frame_count": len(nonzero),
            "nonzero_frames": nonzero[:8],
            "authored_peak_frame": authored_peak,
            "peak_value": values[authored_peak],
        }
        input_channels.append(diagnostic)
        _BASE.check(
            assertions,
            "impulse_fixture",
            f"{name}/channel-{channel}",
            fixture_ok,
            diagnostic,
            {"nonzero_frames": [authored_peak], "peak_value": expected_value},
        )

    values = {
        "fixture_kind": "impulse",
        "actual_rate": {
            "input_frames": case["input_frames"],
            "output_frames": case["output_frames"],
            "speed": case["input_frames"] / case["output_frames"],
        },
        "input_channels": input_channels,
        "outputs": {},
        "quality_scope": "diagnostic only except finite, bounded, and nonzero energy",
    }
    for consumer, output in outputs.items():
        diagnostics = []
        for channel in range(CHANNELS):
            energy, peak_frame, peak_value = _channel_energy_and_peak(output, channel)
            channel_values = {
                "channel": channel,
                "energy": energy,
                "peak_frame": peak_frame,
                "peak_value": peak_value,
            }
            diagnostics.append(channel_values)
            _BASE.check(
                assertions,
                "impulse_nonzero_energy",
                f"{name}/{consumer}/channel-{channel}",
                energy > 0,
                energy,
                "> 0",
            )
        values["outputs"][consumer] = diagnostics

    if case["input_frames"] == case["output_frames"] and case["pitch_semitones"] == 0:
        preview = outputs["preview"]
        for channel in range(CHANNELS):
            _, peak_frame, _ = _channel_energy_and_peak(preview, channel)
            _BASE.check(
                assertions,
                "unity_impulse_onset",
                f"{name}/channel-{channel}",
                peak_frame == authored_peak,
                peak_frame,
                authored_peak,
            )
        identity_error = _BASE.normalized_difference(original, preview)
        values["unity_identity_normalized_rms_error"] = identity_error
        values["quality_scope"] = "unity onset and identity checked; other transforms diagnostic only"
        _BASE.check(
            assertions,
            "unity_identity",
            name,
            identity_error <= TARGETS["identity_normalized_rms_max"],
            identity_error,
            TARGETS["identity_normalized_rms_max"],
        )
    return values


def _analyze_cases(native: dict, pcm: Path) -> dict:
    """Analyze already-validated cases, also allowing compact unit fixtures.

    Returns baseline-style ``targets``, ``measurements``, ``assets``,
    ``assertions``, per-category counts, and total passed/failed counts. Invalid
    PCM lengths or nonfinite samples raise ``ValueError`` like the baseline
    analyzer; measured target misses are retained as failed assertions.
    """

    assets: list[dict] = []
    assertions: list[dict] = []
    measurements: list[dict] = []
    loaded: dict[tuple[Path, int], array] = {}

    def inspect(path: Path, frames: int) -> array:
        key = (path, frames)
        if key not in loaded:
            loaded[key] = _BASE.inspect_pcm(path, frames, assets, assertions)
        return loaded[key]

    _BASE.check(
        assertions,
        "canonical_format",
        "sample_rate",
        native["sample_rate"] == RATE,
        native["sample_rate"],
        RATE,
    )
    _BASE.check(
        assertions,
        "canonical_format",
        "channels",
        native["channels"] == CHANNELS,
        native["channels"],
        CHANNELS,
    )

    for case in native["cases"]:
        name = case["name"]
        input_frames = case["input_frames"]
        output_frames = case["output_frames"]
        original = inspect(pcm / case["input_file"], input_frames)
        outputs = {
            consumer: inspect(pcm / f"{name}-{consumer}.f32", output_frames)
            for consumer in ("preview", "irregular", "export")
        }

        render_dsp_calls = {}
        for consumer in ("preview", "irregular", "export"):
            stats = case["renders"][consumer]
            _check_stats(assertions, name, consumer, stats)
            render_dsp_calls[consumer] = stats["dsp_calls"]
        for consumer in ("irregular", "export"):
            _BASE.check(
                assertions,
                "consumer_dsp_schedule",
                f"{name}/{consumer}",
                render_dsp_calls[consumer] == render_dsp_calls["preview"],
                render_dsp_calls,
                {"all_equal": True},
            )

        comparisons = {
            consumer: _compare(
                assertions,
                "canonical_consumer_equivalence",
                f"{name}/preview-vs-{consumer}",
                outputs["preview"],
                outputs[consumer],
            )
            for consumer in ("irregular", "export")
        }

        seek_measurements = []
        for index, sought in enumerate(case["seeks"]):
            start = sought["start_frame"]
            frames = sought["frames"]
            within = 0 <= start and 0 < frames and start + frames <= output_frames
            _BASE.check(
                assertions,
                "seek_window_within_render",
                f"{name}/{index}",
                within,
                {"start_frame": start, "frames": frames, "output_frames": output_frames},
                "0 <= start_frame < start_frame + frames <= output_frames",
            )
            reference = outputs["preview"][CHANNELS * start : CHANNELS * (start + frames)]
            seek_values = {"start_frame": start, "frames": frames}
            for mode in ("replay", "cached"):
                _check_stats(assertions, name, f"{mode}-{index}", sought[mode])
                actual = inspect(pcm / f"{name}-{mode}-{index}.f32", frames)
                seek_values[mode] = _compare(
                    assertions,
                    f"seek_{mode}_equivalence",
                    f"{name}/{index}",
                    reference,
                    actual,
                )
            seek_measurements.append(seek_values)

        if case["fixture_kind"] == "mixed":
            values = _mixed_measurements(case, original, outputs["preview"], assertions)
        elif case["fixture_kind"] == "impulse":
            values = _impulse_measurements(case, original, outputs, assertions)
        else:
            raise ValueError(f"{name}: unknown fixture_kind {case['fixture_kind']!r}")
        values.update(
            {
                "name": name,
                "consumer_comparisons": comparisons,
                "render_dsp_calls": render_dsp_calls,
                "seeks": seek_measurements,
                "render_performance": {
                    consumer: {
                        "calls": case["renders"][consumer]["calls"],
                        "milliseconds": case["renders"][consumer]["milliseconds"],
                        "maximum_call_ms": case["renders"][consumer]["maximum_call_ms"],
                        "seconds_per_output_second": case["renders"][consumer]["milliseconds"]
                        / 1000
                        / (output_frames / RATE),
                    }
                    for consumer in ("preview", "irregular", "export")
                },
            }
        )
        measurements.append(values)

    categories: dict[str, dict[str, int]] = {}
    for assertion in assertions:
        group = categories.setdefault(assertion["category"], {"passed": 0, "failed": 0})
        group["passed" if assertion["passed"] else "failed"] += 1
    return {
        "targets": TARGETS,
        "measurements": measurements,
        "assets": assets,
        "assertions": assertions,
        "categories": categories,
        "assertions_passed": sum(assertion["passed"] for assertion in assertions),
        "assertions_failed": sum(not assertion["passed"] for assertion in assertions),
    }


def analyze(native: dict, pcm: Path, expected_window: str) -> dict:
    """Validate and analyze a complete production canonical-probe result.

    Manifest/configuration errors raise ``ValueError`` before any PCM file is
    opened. PCM length/nonfinite errors also raise ``ValueError``; measured
    quality or resource misses remain failed assertions in the returned report.
    """

    _validate_manifest(native, expected_window)
    return _analyze_cases(native, pcm)
