#!/usr/bin/env python3
"""Exercise the production encoder with independently decoded bounded fixtures."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from fractions import Fraction
import json
from pathlib import Path
import traceback

from encoder_oracle import (
    CaseSpec, _integer, _ratio, _records, audio_boundary, inspect_case, movie_timescale,
)
from mp4_boxes import inspect_mp4
from native_audio_oracle import exact_time, inspect_native_audio
from qualify_encoder import EncoderHarness, read_pcm


ROOT = Path(__file__).resolve().parent


@dataclass(frozen=True)
class RangeCaseSpec(CaseSpec):
    project_start: int = 0

    def __post_init__(self):
        if type(self.project_start) is not int or not 0 <= self.project_start <= 240:
            raise ValueError("fixture project start exceeds bound")
        super().__post_init__()

    @property
    def audio_samples(self):
        return (audio_boundary(self.project_start + self.frame_count, self.fps_num, self.fps_den)
                - audio_boundary(self.project_start, self.fps_num, self.fps_den))


def inspect_gops(video, fresh):
    """Compare full decoded suffixes; no key flag is accepted as pixel proof."""
    frames = video["frames"]
    boundaries = fresh["boundaries"]
    expected = [frame["pts"] for frame in frames if frame["keyframe"]]
    checks = [{"label": "every observed key boundary was independently decoded",
               "passed": (bool(frames) and bool(expected) and fresh["time_base"] == video["time_base"]
                          and fresh["boundary_count"] == len(boundaries)
                          and [row["requested_pts"] for row in boundaries] == expected)}]
    for row in boundaries:
        suffix = [frame for frame in frames if frame["pts"] >= row["requested_pts"]]
        actual = row["frames"]
        checks.append({"label": f"fresh GOP {row['requested_pts']}: exact decoded suffix",
                       "passed": (len(actual) == row["frame_count"] == len(suffix) and bool(actual)
                                  and all(all(a[key] == b[key] for key in ("pts", "duration", "md5"))
                                          and a["decode_error_flags"] == 0 and not (a["flags"] & 1)
                                          for a, b in zip(actual, suffix))),
                       "decoded_frames": len(actual)})
    return checks


def _single(rows, label):
    if len(rows) != 1:
        raise ValueError(f"expected exactly one {label}, observed {len(rows)}")
    return rows[0]


def inspect_edits(spec, case):
    """Admit only the measured simple delay/reorder edits of this output path.

    Segment durations use the movie clock; media offsets use each track's own
    clock. AAC physical priming remains visible in the manual decode. No event
    location supplies an offset and no PCM is shifted, trimmed or realigned.
    """
    boxes = case["boxes"]
    all_boxes = _records(boxes["boxes"], "MP4 boxes", 65536)
    edits = _records(boxes["edit_lists"], "edit lists", 4096)
    containers = _records(boxes["edit_containers"], "edit containers", 4096)
    tracks = [row for row in all_boxes if row["type"] == "trak"]
    if len(tracks) != 2 or len(edits) != 2 or len(containers) != 2 or boxes["edit_entry_count"] != 2:
        raise ValueError("exactly two tracks, each with one media edit, are required")
    scale = movie_timescale(spec.fps_num, spec.fps_den)
    checks, kinds = [], set()
    packets = _records(case["packets"]["packets"], "packets", 32768)
    for track in tracks:
        if track["path"] != ["moov", "trak"]:
            raise ValueError("media tracks must be direct movie children")
        media = _single([row for row in all_boxes if row["type"] == "mdia"
                         and row["parent_offset"] == track["offset"]], "track media container")
        handler = _single([row for row in boxes["handlers"]
                           if row["parent_offset"] == media["offset"]], "track media handler")
        kind = {"vide": "video", "soun": "audio"}.get(handler["handler_type"])
        if kind is None or kind in kinds:
            raise ValueError("one video and one audio media handler are required")
        kinds.add(kind)
        header = _single([row for row in boxes["timescales"] if row["type"] == "mdhd"
                          and row["parent_offset"] == media["offset"]], f"{kind} media clock")
        media_scale = _integer(header["timescale"], "media timescale", 1, (1 << 32) - 1)
        media_duration = _integer(header["duration_ticks"], "media duration", 1, (1 << 64) - 1)
        if header["duration_unknown"] is not False:
            raise ValueError("media duration must be known")
        container = _single([row for row in containers if row["parent_offset"] == track["offset"]],
                            f"{kind} edit container")
        edit = _single([row for row in edits if row["parent_offset"] == container["offset"]],
                       f"{kind} edit list")
        entries = _records(edit["entries"], "edit entries", 4096)
        if edit["entry_count"] != 1 or len(entries) != 1:
            raise ValueError(f"{kind} requires one simple media edit")
        entry = entries[0]
        offset = _integer(entry["media_time"], "edit media time", 0)
        duration = _integer(entry["segment_duration"], "edit duration", 1, (1 << 64) - 1)
        if (type(entry["media_rate_integer"]) is not int or entry["media_rate_integer"] != 1
                or type(entry["media_rate_fraction"]) is not int or entry["media_rate_fraction"] != 0):
            raise ValueError(f"{kind} edit must have normal rate, without dwell or speed changes")
        expected_duration = (spec.frame_count * spec.frame_duration if kind == "video"
                             else Fraction(spec.audio_samples, 48000))
        selected = [row for row in packets if row["codec_type"] == kind]
        if not selected:
            raise ValueError(f"{kind} has no measured packets")
        timing = [(_integer(row["pts"], "packet PTS") * _ratio(row["time_base"], "packet time base"),
                   _integer(row["dts"], "packet DTS") * _ratio(row["time_base"], "packet time base"),
                   _integer(row["duration"], "packet duration", 1) * _ratio(row["time_base"], "packet time base"))
                  for row in selected]
        observation = case["video"] if kind == "video" else case["audio"]["manual"]
        checks += [
            {"label": f"{kind}: exact edit duration on the common movie clock",
             "passed": Fraction(duration, scale) == expected_duration},
            {"label": f"{kind}: media clock and packet stream match the decoded track",
             "passed": (Fraction(1, media_scale) == _ratio(observation["time_base"], "decoded time base")
                        and all(row["stream_index"] == observation["stream_index"]
                                and _ratio(row["time_base"], "packet time base") == Fraction(1, media_scale)
                                for row in selected))},
            {"label": f"{kind}: measured packet presentation ends at the authored endpoint",
             "passed": max(pts + length for pts, _, length in timing) == expected_duration},
        ]
        first_pts, first_dts, _ = timing[0]
        if kind == "audio":
            delay = _integer(case["encoded"]["report"]["info"]["audio_initial_padding"],
                             "AAC initial padding", 0, 8192)
            delay_time = Fraction(delay, 48000)
            checks.append({"label": "audio: media edit matches measured AAC priming without PCM shift",
                           "passed": (Fraction(offset, media_scale) == delay_time
                                      and first_pts == first_dts == -delay_time
                                      and selected[0]["skip"]["leading"] == delay
                                      and case["audio"]["manual"]["first_sample_pts"] == -delay
                                      and case["audio"]["ordinary"]["first_sample_pts"] == 0
                                      and Fraction(media_duration, media_scale) == expected_duration + delay_time),
                           "media_offset_ticks": offset, "priming_samples": delay})
        else:
            reorder = Fraction(offset, media_scale)
            checks.append({"label": "video: media edit matches only the measured initial reordering",
                           "passed": (first_pts == 0 and first_dts == -reorder
                                      and 0 <= reorder <= spec.requested_b_frames * spec.frame_duration
                                      and (reorder / spec.frame_duration).denominator == 1
                                      and Fraction(media_duration, media_scale) == expected_duration),
                           "media_offset_ticks": offset, "reorder_seconds": str(reorder)})
    return checks


def inspect_native_case(spec, case, output_bytes):
    """The runner's complete acceptance predicate, also used by pure tests."""
    checks = []
    failure = None
    try:
        boxes = case["boxes"]
        report = case["encoded"]["report"]
        ffmpeg = case["ffmpeg_checks"]
        native = case["avfoundation"]["checks"]
        observation = case["avfoundation"]["observation"]
        checks.extend(inspect_gops(case["video"], case["gops"]))
        frames = _records(case["video"]["frames"], "decoded video", 240)
        keys = [index for index, frame in enumerate(frames) if frame["keyframe"] is True]
        intervals = [right - left for left, right in zip(keys, keys[1:] + [len(frames)])]
        requested_gop = max(1, (spec.fps_num + spec.fps_den) // (2 * spec.fps_den))
        maximum_b, run = 0, 0
        for frame in frames:
            run = run + 1 if frame["type"] == "B" else 0
            maximum_b = max(maximum_b, run)
        movie = [row for row in boxes["timescales"] if row["path"] == ["moov", "mvhd"]]
        checks += [
            {"label": "FFmpeg and AVFoundation acceptance checks pass",
             "passed": ffmpeg["passed"] is True and native["passed"] is True and native["outcome"] == "passed"},
            {"label": "muxed movie uses the exact common timescale",
             "passed": len(movie) == 1 and movie[0]["timescale"] == movie_timescale(spec.fps_num, spec.fps_den)},
            {"label": "movie duration retains both exact authored endpoints",
             "passed": (len(movie) == 1 and movie[0]["duration_unknown"] is False
                        and movie[0]["duration_ticks"] == max(spec.frame_count * spec.frame_duration,
                                                              Fraction(spec.audio_samples, 48000))
                        * movie_timescale(spec.fps_num, spec.fps_den))},
            {"label": "presented video and audio streams start at output zero",
             "passed": (case["video"]["stream_start_pts"] == 0
                        and all(case["audio"][mode]["stream_start_pts"] == 0 for mode in ("ordinary", "manual"))
                        and exact_time(observation["track"]["time_range"]["start"], "audio track start") == 0)},
            {"label": "observed GOP intervals remain within one frame of the half-second target",
             "passed": (bool(keys) and keys[0] == 0 and bool(intervals)
                        and max(intervals) <= requested_gop + 1)},
            {"label": "requested B-frame path actually reorders pictures",
             "passed": (maximum_b <= spec.requested_b_frames
                        and (spec.requested_b_frames == 0 or spec.frame_count <= requested_gop or maximum_b > 0))},
            {"label": "all three readers qualify absolute event timing",
             "passed": (ffmpeg["observations"]["ordinary"]["event_timing_qualified"] is True
                        and ffmpeg["observations"]["manual"]["event_timing_qualified"] is True
                        and native["event_timing_qualified"] is True)},
            {"label": "one fast-start movie and exactly two tracks",
             "passed": (boxes["fast_start"]["moov_count"] == 1 and boxes["fast_start"]["mdat_count"] >= 1
                        and boxes["fast_start"]["moov_before_mdat"] is True
                        and sum(row["path"] == ["moov", "trak"] for row in boxes["boxes"]) == 2)},
            {"label": "exact supplied native frame/sample counts",
             "passed": report["video_frames"] == spec.frame_count and report["audio_samples"] == spec.audio_samples},
            {"label": "both codecs reached EOF", "passed": report["video_eof"] is True and report["audio_eof"] is True},
            {"label": "fast-start read used only its owned descriptor",
             "passed": report["faststart_read_opens"] == 1 and report["faststart_read_closes"] == 1},
            {"label": "reported output extent matches actual file", "passed": report["output_bytes"] == output_bytes},
        ]
        checks.extend(inspect_edits(spec, case))
    except (ValueError, KeyError, TypeError, IndexError, OverflowError) as error:
        failure = str(error)
        checks.append({"label": "valid complete boundary observations", "passed": False, "details": failure})
    return {"passed": bool(checks) and all(row["passed"] for row in checks), "checks": checks, "failure": failure}


class NativeEncodeHarness(EncoderHarness):
    def __init__(self, work, sanitizers, build_report, encoder):
        super().__init__(work, sanitizers, build_report)
        self.encoder = encoder.resolve()
        self.native_audio = self.work / "avfoundation_probe"
        self.report["scope"] = ("production descriptor-only native encode boundary, synthetic I420/PCM; "
                                "no project export, destination publication or release qualification")

    def prepare(self):
        super().prepare()
        artifact = self.artifact(self.encoder)
        self.report["production_encoder"] = artifact
        self.admitted_files[str(self.encoder)] = artifact["sha256"]
        linked = self.run(["otool", "-L", self.encoder]).stdout
        self.report["production_encoder"]["linked_libraries"] = linked
        self.admit_linkage(linked)
        flags = ["-std=c11", "-O2", "-g", "-fobjc-arc", "-Wall", "-Wextra", "-Werror",
                 "-arch", "arm64", "-mmacosx-version-min=15.0"]
        if self.sanitizers:
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
        self.run(["xcrun", "--sdk", "macosx", "clang", *flags, ROOT / "avfoundation_probe.m",
                  "-o", self.native_audio, "-framework", "Foundation", "-framework", "AVFoundation",
                  "-framework", "CoreMedia", "-framework", "AudioToolbox"])
        artifact = self.artifact(self.native_audio)
        self.report["avfoundation_reader"] = artifact
        self.admitted_files[str(self.native_audio)] = artifact["sha256"]

    def capture(self, name, mode, b_policy, spec):
        case = {"name": name, "mode": mode, "b_policy": b_policy, "spec": vars(spec), "passed": False}
        self.report["cases"].append(case)
        path = self.work / f"{name}.mp4"
        try:
            encoded = self.run([self.encoder, path, mode, b_policy, spec.fps_num, spec.fps_den,
                                spec.frame_count, spec.project_start, spec.pcm_kind], required=False)
            case["encode_exit_code"] = encoded.returncode
            if encoded.returncode:
                case["failure"] = "requested encoder configuration failed; no completed output admitted"
                # Retain the measured M5 Max/VT rejection as a failed capability,
                # never reinterpret it as a successfully encoded path.
                case["expected_capability_rejection"] = (mode == "hardware" and b_policy == "two"
                    and encoded.returncode == 1 and not encoded.stdout.strip()
                    and " < dts (" in encoded.stderr and "mux encoded packet: Invalid argument" in encoded.stderr)
                return
            case["encoded"] = json.loads(encoded.stdout)
            case["boxes"] = inspect_mp4(path)
            case["video"] = json.loads(self.run([self.binary, "video", path]).stdout)
            case["packets"] = json.loads(self.run([self.binary, "packets", path]).stdout)
            case["gops"] = json.loads(self.run([self.binary, "gops", path]).stdout)
            case["ffprobe"] = json.loads(self.run(["ffprobe", "-v", "error", "-show_streams", "-show_format",
                                                   "-print_format", "json", path]).stdout)
            case["audio"] = {}
            pcm = {}
            for decoder in ("ordinary", "manual"):
                output = self.work / f"{name}.{decoder}.f32"
                value = json.loads(self.run([self.binary, "audio", path, output, decoder]).stdout)
                value["pcm"] = self.artifact(output)
                case["audio"][decoder] = value
                pcm[decoder] = read_pcm(output)
            tolerance = (48000 * spec.fps_den + spec.fps_num - 1) // spec.fps_num - 1
            case["declared_tolerance_samples"] = tolerance
            event_radius = 64 if spec.frame_count == 1 else 4096
            case["event_search_radius_samples"] = event_radius
            result = inspect_case(spec, case["encoded"]["source"], case["video"], case["audio"]["ordinary"],
                                  case["audio"]["manual"], case["ffprobe"], ordinary_pcm=pcm["ordinary"],
                                  manual_pcm=pcm["manual"], tolerance_samples=tolerance, packets=case["packets"],
                                  event_search_radius=event_radius)
            case["ffmpeg_checks"] = result
            output = self.work / f"{name}.avfoundation.f32"
            observation = json.loads(self.run([self.native_audio, path, output]).stdout)
            case["avfoundation"] = {"observation": observation, "pcm": self.artifact(output),
                                     "checks": inspect_native_audio(spec, observation, read_pcm(output),
                                                                    tolerance_samples=tolerance,
                                                                    event_search_radius=event_radius)}
            admission = inspect_native_case(spec, case, path.stat().st_size)
            case["boundary_checks"] = admission["checks"]
            case["passed"] = admission["passed"]
            if admission["failure"] is not None:
                case["failure"] = admission["failure"]
        except (ValueError, KeyError, OSError, RuntimeError, AssertionError) as error:
            case["failure"] = str(error)
        finally:
            if path.exists():
                case["output"] = self.artifact(path)

    def fault(self, fault):
        path = self.work / f"fault-{fault}.mp4"
        result = self.run([self.encoder, path, "hardware", "none", 60, 1, 120, 0, "impulses", fault], required=False)
        expected = {
            "byte-limit": ("packet_limit", "output_too_large"),
            "cancel": ("encoding was cancelled",),
            "ordinal": ("picture is not the next exact output timestamp",),
            "audio-hole": ("audio is not the next exact contiguous output block",),
            "nonfinite": ("audio input contains a nonfinite sample",),
            "incomplete": ("finish requires every captured picture and audio sample",),
        }[fault]
        case = {"name": f"fault-{fault}", "exit_code": result.returncode,
                "passed": (result.returncode == 1 and not result.stdout.strip()
                           and any(text in result.stderr for text in expected)),
                "diagnostic": result.stderr, "expected_diagnostic": expected}
        if path.exists():
            case["retained_partial"] = self.artifact(path)
        self.report["cases"].append(case)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--build-report", required=True, type=Path)
    parser.add_argument("--encoder", required=True, type=Path)
    parser.add_argument("--sanitizers", action="store_true")
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()):
        parser.error("work directory must be empty")
    harness = NativeEncodeHarness(work, args.sanitizers, args.build_report.resolve(), args.encoder)
    try:
        harness.prepare()
        for mode in ("hardware", "software"):
            for b_policy, count in (("two", 2), ("none", 0)):
                harness.capture(f"{mode}-{b_policy}-ntsc", mode, b_policy,
                                RangeCaseSpec(frame_count=90, requested_b_frames=count))
                harness.capture(f"{mode}-{b_policy}-60", mode, b_policy,
                                RangeCaseSpec(frame_count=120, fps_num=60, fps_den=1, requested_b_frames=count))
        harness.capture("hardware-two-edges", "hardware", "two",
                        RangeCaseSpec(frame_count=120, fps_num=60, fps_den=1, requested_b_frames=2, pcm_kind="edges"))
        harness.capture("hardware-none-edges", "hardware", "none",
                        RangeCaseSpec(frame_count=120, fps_num=60, fps_den=1, pcm_kind="edges"))
        harness.capture("software-two-edges", "software", "two",
                        RangeCaseSpec(frame_count=120, fps_num=60, fps_den=1, requested_b_frames=2, pcm_kind="edges"))
        harness.capture("hardware-none-one60", "hardware", "none",
                        RangeCaseSpec(frame_count=1, fps_num=60, fps_den=1))
        harness.capture("hardware-none-nonzero", "hardware", "none",
                        RangeCaseSpec(frame_count=1, project_start=1))
        for fault in ("byte-limit", "cancel", "ordinal", "audio-hole", "nonfinite", "incomplete"):
            harness.fault(fault)
        passed = all(row["passed"] or row.get("expected_capability_rejection", False)
                     for row in harness.report["cases"])
        harness.report["result"] = "passed scoped paths with hardware B-frame rejection" if passed else "failed scoped checks"
    except Exception as error:
        harness.report["failure"] = str(error)
        harness.report["traceback"] = traceback.format_exc()
    finally:
        harness.finish_admission()
        if harness.process_faults:
            harness.report["result"] = "failed: process or sanitizer fault"
        path = work / "report.json"
        path.write_text(json.dumps(harness.report, indent=2) + "\n")
        print(json.dumps({"report": str(path), "result": harness.report["result"],
                          "cases": [{"name": row["name"], "passed": row["passed"], "failure": row.get("failure")}
                                    for row in harness.report["cases"]]}), flush=True)
    raise SystemExit(0 if harness.report["result"] == "passed scoped paths with hardware B-frame rejection" else 1)


if __name__ == "__main__":
    main()
