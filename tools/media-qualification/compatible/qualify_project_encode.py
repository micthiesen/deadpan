#!/usr/bin/env python3
"""Independently decode real project candidates and compare retained direct inputs."""

import argparse
import json
import os
from pathlib import Path
import stat
import traceback

from encoder_oracle import (
    PcmSpan, _Checks, _metadata_checks, _packet_checks, inspect_pcm_events, movie_timescale,
)
from mp4_boxes import inspect_mp4
from native_audio_oracle import _adapt, exact_time
from project_encode_oracle import compare_pcm, compare_plane, ffmpeg_spans, inspect_video
from qualify_encoder import read_pcm
from qualify_native_encode import NativeEncodeHarness, RangeCaseSpec, inspect_edits, inspect_gops


def admit_file(path, maximum, expected=None):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        metadata = os.fstat(descriptor)
        if (not stat.S_ISREG(metadata.st_mode) or not 0 < metadata.st_size <= maximum
                or expected is not None and metadata.st_size != expected):
            raise ValueError(f"fixture file extent or type is invalid: {path}")
    finally:
        os.close(descriptor)


def read_capture(path):
    maximum = 32 * 1024 * 1024
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
    with os.fdopen(descriptor, 'rb') as file:
        before = os.fstat(file.fileno())
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= maximum:
            raise ValueError("project capture report exceeds its regular-file byte bound")
        data = file.read(maximum + 1)
        after = os.fstat(file.fileno())
        if len(data) != before.st_size or before.st_mtime_ns != after.st_mtime_ns or before.st_ctime_ns != after.st_ctime_ns:
            raise ValueError("project capture report changed during read")
    report = json.loads(data)
    cases = report['encoded']['cases']
    expected = {'structural', 'nonzero', 'software-two', 'odd', 'marker', 'generated', 'after-cancel'}
    if not isinstance(cases, list) or len(cases) != len(expected) or {case['name'] for case in cases} != expected:
        raise ValueError("project capture requires exactly seven unique cases")
    return report


class ProjectEncodeHarness(NativeEncodeHarness):
    def capture_project(self, captured):
        name = captured["name"]
        case = {"name": name, "passed": False, "captured": captured}
        self.report["cases"].append(case)
        contract = captured["contract"]
        fps = contract["frame_rate"]
        spec = RangeCaseSpec(frame_count=contract["frame_count"], fps_num=fps["numerator"],
                            fps_den=fps["denominator"], project_start=contract["range"]["start"],
                            width=contract["raster"][0], height=contract["raster"][1],
                            requested_b_frames=2 if captured["manifest"]["contract"]["choice"]["b_frames"] == "target_two" else 0)
        path = Path(captured["path"])
        references = [Path(captured[key]) for key in ("picture_reference", "audio_reference")]
        admit_file(path, 64 * 1024 * 1024, captured["manifest"]["movie"]["byte_length"])
        admit_file(references[0], 512 * 1024 * 1024, spec.frame_count * spec.width * spec.height * 3 // 2)
        admit_file(references[1], 192000 * 8, spec.audio_samples * 8)
        for file in [path, *references]:
            record = self.artifact(file)
            self.admitted_files[str(file.resolve())] = record["sha256"]
        checks = _Checks()
        case["spec"] = vars(spec)
        case["output"] = self.artifact(path)
        case["encoded"] = {"report": captured["manifest"]["report"]}
        declared = captured["manifest"]["movie"]
        checks.add("retained candidate has independently verified SHA and exact extent",
                   case["output"]["sha256"] == declared["sha256"]
                   and path.stat().st_size == declared["byte_length"])
        checks.add("origin-based exact PCM count",
                   spec.audio_samples == contract["project_audio_end"] - contract["project_audio_start"])
        raw_path = self.work / f"{name}.decoded.i420"
        case["video"] = json.loads(self.run([self.binary, "project-video", path, raw_path]).stdout)
        case["packets"] = json.loads(self.run([self.binary, "packets", path]).stdout)
        case["gops"] = json.loads(self.run([self.binary, "project-gops", path]).stdout)
        case["boxes"] = inspect_mp4(path)
        case["ffprobe"] = json.loads(self.run(["ffprobe", "-v", "error", "-show_streams", "-show_format",
                                                "-print_format", "json", path]).stdout)
        case["video_checks"] = inspect_video(spec, case["video"])
        checks.add("all decoded picture clocks and interpretations pass", case["video_checks"]["passed"])
        checks.checks.extend({**row, "diagnostic": False} for row in inspect_gops(case["video"], case["gops"]))
        _metadata_checks(checks, spec, case["ffprobe"])
        _packet_checks(checks, spec, case["packets"])
        planes = []
        luma = spec.width * spec.height
        expected_bytes = spec.frame_count * luma * 3 // 2
        checks.add("complete decoded and reference plane extent",
                   raw_path.stat().st_size == references[0].stat().st_size == expected_bytes
                   and case["video"]["decoded_bytes"] == expected_bytes)
        with raw_path.open('rb') as actual, references[0].open('rb') as reference:
            for ordinal in range(spec.frame_count):
                for plane, length in [('Y', luma), ('Cb', luma // 4), ('Cr', luma // 4)]:
                    comparison = compare_plane(actual.read(length), reference.read(length))
                    planes.append({"ordinal": ordinal, "plane": plane, **comparison})
            if actual.read(1) or reference.read(1):
                raise ValueError("unexpected trailing picture data")
        case["complete_planes"] = planes
        case["decoded_planes"] = self.artifact(raw_path)
        checks.add("every complete plane meets fixed lossy-stage bounds", all(row["passed"] for row in planes))
        expected_pcm = read_pcm(references[1])
        checks.add("complete canonical PCM extent", len(expected_pcm) == spec.audio_samples * 2)
        case["audio"] = {}
        case["pcm_comparisons"] = {}
        event_reports = {}
        for mode in ['ordinary', 'manual']:
            pcm_path = self.work / f"{name}.{mode}.f32"
            observation = json.loads(self.run([self.binary, "audio", path, pcm_path, mode]).stdout)
            pcm = read_pcm(pcm_path)
            spans = ffmpeg_spans(observation, pcm)
            case["audio"][mode] = observation
            comparison = compare_pcm(pcm, spans, expected_pcm)
            comparison["artifact"] = self.artifact(pcm_path)
            case["pcm_comparisons"][mode] = comparison
            checks.add(f"{mode}: all authored PCM at unmodified absolute PTS", comparison["passed"])
            checks.add(f"{mode}: presented audio starts at zero", observation["stream_start_pts"] == 0)
            if name == "marker":
                event_reports[mode] = inspect_pcm_events(spec, pcm, spans, tolerance_samples=0, label=mode)
        native_path = self.work / f"{name}.avfoundation.f32"
        observation = json.loads(self.run([self.native_audio, path, native_path]).stdout)
        pcm = read_pcm(native_path)
        native_checks = _Checks()
        spans, start, duration = _adapt(native_checks, observation, pcm)
        comparison = compare_pcm(pcm, spans, expected_pcm)
        comparison["artifact"] = self.artifact(native_path)
        case["pcm_comparisons"]["avfoundation"] = comparison
        case["avfoundation"] = {"observation": observation, "checks": native_checks.result()}
        checks.add("AVFoundation exact track start/duration and source hash",
                   start == 0 and duration == spec.audio_samples
                   and observation["input_sha256"] == case["output"]["sha256"])
        checks.add("AVFoundation complete absolute PCM fidelity", comparison["passed"])
        checks.add("AVFoundation bounded format/timing admission", native_checks.result()["passed"])
        if name == "marker":
            case["marker_video"] = json.loads(self.run([self.binary, "video", path]).stdout)
            checks.add("every decoded visible marker keeps its authored frame number",
                       [frame["authored_identity"] for frame in case["marker_video"]["frames"]]
                       == list(range(spec.frame_count)))
            event_reports["avfoundation"] = inspect_pcm_events(spec, pcm, spans, tolerance_samples=0, label="native")
            event_reports["canonical"] = inspect_pcm_events(spec, expected_pcm,
                [PcmSpan(0, spec.audio_samples, 0)], tolerance_samples=0, label="canonical")
            for reader, result in event_reports.items():
                checks.add(f"{reader}: independently authored events are exact",
                           result["passed"] and result["observations"]["event_timing_qualified"]
                           and all(event["error_samples"] == 0 for event in result["observations"]["events"]))
            case["marker_events"] = event_reports
        checks.checks.extend({**row, "diagnostic": False} for row in inspect_edits(spec, case))
        boxes = case["boxes"]
        movie = [row for row in boxes["timescales"] if row["path"] == ["moov", "mvhd"]]
        scale = movie_timescale(spec.fps_num, spec.fps_den)
        checks.add("exact common movie clock, duration and fast-start order",
                   len(movie) == 1 and movie[0]["timescale"] == scale
                   and movie[0]["duration_unknown"] is False
                   and movie[0]["duration_ticks"] == max(spec.frame_count * spec.frame_duration,
                       exact_time(observation["track"]["time_range"]["duration"])) * scale
                   and boxes["fast_start"]["moov_count"] == 1
                   and boxes["fast_start"]["mdat_count"] >= 1 and boxes["fast_start"]["moov_before_mdat"] is True)
        case["checks"] = checks.checks
        case["passed"] = checks.result()["passed"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', required=True, type=Path)
    parser.add_argument('--build-report', required=True, type=Path)
    parser.add_argument('--worker', required=True, type=Path)
    parser.add_argument('--picture-report', required=True, type=Path)
    parser.add_argument('--sanitizers', action='store_true')
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    if any(args.work.iterdir()):
        parser.error('work directory must be empty')
    harness = ProjectEncodeHarness(args.work.resolve(), args.sanitizers, args.build_report.resolve(), args.worker)
    harness.report['scope'] = 'actual committed project MP4 candidates, direct full-plane and canonical PCM comparisons; not publication or release acceptance'
    try:
        harness.prepare()
        report = read_capture(args.picture_report)
        harness.report['project_capture'] = harness.artifact(args.picture_report)
        harness.admitted_files[str(args.picture_report.resolve())] = harness.report['project_capture']['sha256']
        assert report['status'] == 'passed' and report['encoded']['status'].startswith('passed ')
        assert all(check['passed'] for check in report['encoded']['checks'])
        cases = report['encoded']['cases']
        for case in cases:
            try:
                harness.capture_project(case)
            except Exception as error:
                harness.report['cases'][-1]['failure'] = str(error)
                harness.report['cases'][-1]['traceback'] = traceback.format_exc()
        harness.report['result'] = 'passed scoped project encoding' if all(case['passed'] for case in harness.report['cases']) else 'failed scoped project encoding'
    except Exception as error:
        harness.report['failure'] = str(error)
        harness.report['traceback'] = traceback.format_exc()
    finally:
        harness.finish_admission()
        if harness.process_faults:
            harness.report['result'] = 'failed: process or sanitizer fault'
        path = args.work / 'report.json'
        path.write_text(json.dumps(harness.report, indent=2) + '\n')
        print(json.dumps({'report': str(path), 'result': harness.report['result'],
                          'cases': [{'name': row['name'], 'passed': row['passed'], 'failure': row.get('failure')}
                                    for row in harness.report['cases']]}), flush=True)
    raise SystemExit(0 if harness.report['result'] == 'passed scoped project encoding' else 1)


if __name__ == '__main__':
    main()
