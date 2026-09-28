#!/usr/bin/env python3
"""Qualify two real Metal-rendered I420 frames through pinned hardware H.264."""

import argparse
from datetime import datetime, timezone
from fractions import Fraction
import json
from pathlib import Path
import re

from encoder_oracle import OracleError, _effective_frame_sar
from mp4_boxes import inspect_mp4
from qualify_encoder import EncoderHarness, source_inventory
from qualify_native_audio import bounded_file, strict_json


ROOT = Path(__file__).resolve().parent
PASS = "passed scoped renderer-plane H.264 checks"
COLOR = {"primaries": "bt709", "transfer": "bt709", "matrix": "bt709", "range": "tv", "chroma_location": "left"}
MAX_ERROR, MAX_MEAN = 12, 2


def compare_planes(actual, expected, width, height, maximum=MAX_ERROR, mean=MAX_MEAN):
    if (type(width) is not int or type(height) is not int or width not in (318, 320) or height != 180
            or len(actual) != width * height * 3 // 2 or len(expected) != len(actual)):
        raise ValueError("exact full tight-I420 fixture sizes required")
    result, offset = [], 0
    for name, w, h in (("Y", width, height), ("Cb", width // 2, height // 2), ("Cr", width // 2, height // 2)):
        count, total, peak, failures, examples = w * h, 0, 0, 0, []
        for index in range(count):
            value, reference = actual[offset + index], expected[offset + index]
            difference = abs(value - reference)
            total += difference
            peak = max(peak, difference)
            if difference > maximum:
                failures += 1
                if len(examples) < 12:
                    examples.append({"x": index % w, "y": index // w, "byte_offset": offset + index,
                                     "actual": value, "expected": reference, "difference": difference})
        result.append({"plane": name, "width": w, "height": h, "compared_codes": count,
                       "maximum_error": peak, "mean_absolute_error": total / count,
                       "out_of_tolerance_codes": failures, "first_failures": examples,
                       "passed": failures == 0 and total <= mean * count})
        offset += count
    return {"passed": all(plane["passed"] for plane in result), "compared_codes": offset,
            "maximum_tolerance_codes": maximum, "mean_tolerance_codes": mean, "planes": result}


def renderer_cases(report):
    if (not isinstance(report, dict) or report.get("schema_version") != 1 or report.get("status") != "passed"
            or not isinstance(report.get("adapter"), dict) or report["adapter"].get("backend") != "Metal"
            or report.get("case_count") != 2 or not isinstance(report.get("checks"), list)
            or not report["checks"] or any(not isinstance(row, dict) or row.get("passed") is not True for row in report["checks"])):
        raise ValueError("successful real Metal renderer qualification report required")
    cases = report.get("cases")
    if (not isinstance(cases, list) or len(cases) != 2
            or any(not isinstance(case, dict) or type(case.get("width")) is not int for case in cases)
            or sorted(case["width"] for case in cases) != [318, 320]):
        raise ValueError("exact 320x180 and 318x180 renderer fixture pair required")
    directory = Path(report["fixture_directory"])
    if not directory.is_absolute():
        raise ValueError("renderer fixture directory must be absolute")
    for case in cases:
        width = case["width"]
        name = f"linear-rec709-patches-chroma-{width}x180"
        expected = {"name": name, "height": 180, "pixel_format": "yuv420p", "plane_order": ["Y", "Cb", "Cr"],
                    "frame_rate": [30, 1], "frame_count": 1, "byte_count": width * 180 * 3 // 2,
                    "y_stride_bytes": width, "chroma_stride_bytes": width // 2, "color": COLOR}
        if (any(type(case.get(key)) is not type(value) or case[key] != value for key, value in expected.items())
                or not isinstance(case.get("comparison"), dict) or case["comparison"].get("passed") is not True):
            raise ValueError("renderer plane contract or reference comparison differs")
        for key, suffix, hash_key in (("raw_path", ".i420", "actual_sha256"),
                                      ("reference_path", "-reference.i420", "reference_sha256")):
            path = Path(case[key])
            if not path.is_absolute() or path.parent.resolve() != directory.resolve() or path.name != name + suffix:
                raise ValueError("renderer artifact path differs from exact fixture")
            if not isinstance(case.get(hash_key), str) or re.fullmatch(r"[0-9a-f]{64}", case[hash_key]) is None:
                raise ValueError("renderer report must bind exact plane SHA-256")
    return cases


def check_decode(report, width, height):
    checks = []
    def add(label, passed, actual=None):
        checks.append({"label": label, "passed": bool(passed), "actual": actual})
    add("complete one-frame video-only decoder drain", report.get("schema_version") == 1 and report.get("kind") == "decode_plane"
        and report.get("decoder_drained") is True and report.get("frame_count") == 1
        and report.get("audio_streams") == 0 and len(report.get("frames", [])) == 1)
    if not checks[-1]["passed"]:
        return checks
    frame = report["frames"][0]
    add("High-profile 8-bit 4:2:0 exact geometry", report.get("profile") == "High"
        and frame.get("width") == width and frame.get("height") == height
        and frame.get("pixel_format") == "yuv420p" and frame.get("bit_depth") == 8
        and frame.get("chroma_subsampling") == [2, 2])
    add("Rec709 matrix, transfer, primaries, limited range and left chroma",
        all(frame.get(key) == value for key, value in {"color_range": "tv", "color_space": "bt709",
            "color_transfer": "bt709", "color_primaries": "bt709", "chroma_location": "left"}.items()))
    add("progressive uncorrupted decoded frame", frame.get("interlaced") is False
        and frame.get("decode_error_flags") == 0 and type(frame.get("flags")) is int and not frame["flags"] & 1)
    try:
        sar, selected = _effective_frame_sar(frame.get("sample_aspect_ratio"), report.get("stream_sample_aspect_ratio"))
        add("explicit square pixels", sar == 1, {"field": selected, "value": [sar.numerator, sar.denominator]})
    except OracleError as error:
        add("explicit square pixels", False, {"error": str(error)})
    try:
        tb = report["time_base"]
        if not isinstance(tb, list) or len(tb) != 2 or any(type(value) is not int or value <= 0 for value in tb):
            raise ValueError("positive exact stream clock required")
        clock = Fraction(*tb)
        timing = (all(type(value) is int and value == 0 for value in (frame["pts"], frame["best_effort_pts"], report["stream_start_pts"]))
                  and type(frame["duration"]) is int and type(report["stream_duration"]) is int
                  and frame["duration"] * clock == Fraction(1, 30)
                  and report["stream_duration"] * clock == Fraction(1, 30))
    except (KeyError, TypeError, ValueError, ZeroDivisionError):
        timing = False
    add("exact one-frame 30fps presentation interval", timing)
    add("complete decoded tight plane count", report.get("decoded_bytes") == width * height * 3 // 2)
    return checks


class PictureHarness(EncoderHarness):
    def __init__(self, work, sanitizers, build_report, renderer_report):
        super().__init__(work, sanitizers, build_report)
        self.renderer_report = renderer_report
        self.report.update(scope="real Metal working picture to I420 through pinned H.264, synthetic SDR fixtures only",
                           experiment_completed=False, declared_lossy_tolerance={"maximum_codes": MAX_ERROR,
                           "mean_codes_per_plane": MAX_MEAN, "reason": "declared before measurement for lossy H.264; every pixel is compared"},
                           unqualified=["AAC timing", "multi-frame closed GOPs", "real footage and full resolution",
                                        "HDR", "physical display/listening", "product export integration"])

    def admit_bytes(self, path, expected=None, maximum=64 * 1024 * 1024):
        data, artifact = bounded_file(path, maximum)
        if expected is not None and artifact["sha256"] != expected:
            raise ValueError(f"renderer artifact SHA-256 mismatch: {path}")
        self.admitted_files[str(path)] = artifact["sha256"]
        self.loaded_paths[str(path)] = {"resolved": str(Path(path).resolve()), "sha256": artifact["sha256"]}
        return data, artifact

    def capture_planes(self, fixture):
        name, width, height = fixture["name"], fixture["width"], fixture["height"]
        case = {"name": name, "renderer_fixture": fixture, "status": "failed", "checks": []}
        self.report["cases"].append(case)
        mp4, packet_log, decoded = (self.work / (name + suffix) for suffix in (".mp4", ".packets.jsonl", ".decoded.i420"))
        try:
            actual, case["input"] = self.admit_bytes(Path(fixture["raw_path"]), fixture["actual_sha256"])
            reference, case["reference"] = self.admit_bytes(Path(fixture["reference_path"]), fixture["reference_sha256"])
            case["renderer_reference_comparison"] = compare_planes(actual, reference, width, height, maximum=1, mean=1)
            if not case["renderer_reference_comparison"]["passed"]:
                raise ValueError("admitted renderer planes disagree with independent reference")
            result = self.run([self.binary, "encode-plane", fixture["raw_path"], mp4, packet_log, width, height], required=False)
            case["encode_exit_code"] = result.returncode
            if result.returncode:
                raise ValueError("hardware encoder rejected owned plane input; retained command logs")
            case["encoded"] = strict_json(result.stdout)
            if case["encoded"].get("input_sha256") != fixture["actual_sha256"]:
                raise ValueError("encoder did not consume the admitted actual renderer planes")
            case["checks"].append({"label": "one owned frame encoded and fully drained without audio",
                "passed": case["encoded"].get("schema_version") == 1 and case["encoded"].get("kind") == "encode_plane"
                and case["encoded"].get("encoder_drained") is True and case["encoded"].get("frame_count") == 1
                and case["encoded"].get("packet_count") == 1 and case["encoded"].get("audio_streams") == 0
                and case["encoded"].get("input_bytes") == len(actual)})
            case["boxes"] = inspect_mp4(mp4)
            case["checks"].append({"label": "faststart and structurally absent edit lists",
                "passed": case["boxes"]["fast_start"]["moov_before_mdat"] is True
                and not case["boxes"]["has_edts"] and not case["boxes"]["has_elst"]})
            case["decoded"] = strict_json(self.run([self.binary, "decode-plane", mp4, decoded]).stdout)
            decoded_bytes, _ = bounded_file(decoded, width * height * 3 // 2)
            case["checks"].extend(check_decode(case["decoded"], width, height))
            case["decoded_vs_actual"] = compare_planes(decoded_bytes, actual, width, height)
            case["checks"].append({"label": "every decoded plane code within declared lossy bounds",
                                    "passed": case["decoded_vs_actual"]["passed"]})
            if all(check["passed"] for check in case["checks"]):
                case["status"] = "passed"
        except (AssertionError, ValueError, OSError, RuntimeError, KeyError, TypeError) as error:
            case["failure"] = str(error)
        finally:
            case["artifacts"] = {key: self.artifact(path) for key, path in
                                 (("mp4", mp4), ("before_mux", packet_log), ("decoded", decoded)) if path.is_file()}
        return case

    def measure(self):
        data, self.report["renderer_report"] = self.admit_bytes(self.renderer_report)
        report = strict_json(data)
        fixtures = renderer_cases(report)
        self.report["renderer"] = report
        for fixture in fixtures:
            self.capture_planes(fixture)
        self.report["experiment_completed"] = True
        self.report["result"] = PASS if all(case["status"] == "passed" for case in self.report["cases"]) else "failed: renderer-plane encoding qualification"
        if self.process_faults:
            self.report["result"] = "failed: native process fault"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-report", type=Path, required=True)
    parser.add_argument("--renderer-report", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--sanitizers", action="store_true")
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()) or args.output.exists():
        parser.error("work must be empty and output must be new")
    bounded_file(args.build_report, 64 * 1024 * 1024)
    harness = PictureHarness(work, args.sanitizers, args.build_report.resolve(), args.renderer_report.resolve())
    try:
        harness.prepare()
        harness.measure()
    except (AssertionError, ValueError, OSError, RuntimeError, KeyError, TypeError) as error:
        harness.report["failure"] = str(error)
        harness.report["result"] = "failed: picture export experiment"
    finally:
        harness.finish_admission()
        harness.report["source_sha256"] = source_inventory()
        harness.report["source_unchanged_during_run"] = harness.report["source_sha256"] == harness.report["source_sha256_at_start"]
        if not harness.report["source_unchanged_during_run"]:
            harness.report["result"] = "failed: source changed during run"
        harness.report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("x") as target:
            json.dump(harness.report, target, indent=2, allow_nan=False)
            target.write("\n")
        print(json.dumps({"result": harness.report["result"], "report": str(args.output)}))
    return 0 if harness.report["result"] == PASS else 1


if __name__ == "__main__":
    raise SystemExit(main())
