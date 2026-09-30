import copy
import json
from pathlib import Path
import tempfile
import unittest
from subprocess import CompletedProcess

from qualify_native_encode import RangeCaseSpec, encoded_failure, inspect_gops, inspect_native_case
from encoder_oracle import PcmSpan, inspect_pcm_events, movie_timescale
from mp4_boxes import inspect_mp4
from test_mp4_boxes import box, edit_list, timing


class TypedFailureTests(unittest.TestCase):
    def test_failure_kind_requires_a_complete_failed_envelope(self):
        value = {"status": "failed", "kind": "video_timestamp_order", "diagnostic": "measured rejection"}
        result = CompletedProcess([], 1, json.dumps(value), "")
        self.assertEqual(encoded_failure(result), value)
        for code, body in [(0, value), (2, value), (1, dict(value, status="passed")),
                           (1, dict(value, kind=None)), (1, dict(value, fallback=True)),
                           (1, dict(value, diagnostic=""))]:
            self.assertIsNone(encoded_failure(CompletedProcess([], code, json.dumps(body), "")))
        self.assertIsNone(encoded_failure(CompletedProcess([], 1, "", "video_timestamp_order")))
        misleading = dict(value, kind="io", diagnostic="video_timestamp_order")
        self.assertEqual(encoded_failure(CompletedProcess([], 1, json.dumps(misleading), ""))["kind"], "io")


def boundary_fixture(spec=None):
    spec = spec or RangeCaseSpec(frame_count=90, requested_b_frames=2)
    scale = movie_timescale(spec.fps_num, spec.fps_den)
    gop = max(1, (spec.fps_num + spec.fps_den) // (2 * spec.fps_den))
    video_ticks = spec.frame_count * spec.fps_den
    video_movie_ticks = video_ticks * scale // spec.fps_num
    audio_movie_ticks = spec.audio_samples * scale // 48000
    delay = 1024
    reorder = spec.fps_den if spec.requested_b_frames and spec.frame_count > 1 else 0

    def track(kind, media_scale, media_duration, duration, offset):
        return box(b"trak", box(b"edts", edit_list(entries=((duration, offset, 1, 0),)))
                   + box(b"mdia", timing(b"mdhd", media_scale, media_duration)
                         + box(b"hdlr", bytes(8) + kind + bytes(12))))

    # Audio deliberately precedes video. Qualification must use parent offsets
    # and handlers, not guess that the first edit list belongs to video.
    movie = box(b"moov", timing(b"mvhd", scale, max(video_movie_ticks, audio_movie_ticks))
                + track(b"soun", 48000, spec.audio_samples + delay, audio_movie_ticks, delay)
                + track(b"vide", spec.fps_num, video_ticks, video_movie_ticks, reorder))
    data = movie + box(b"mdat", b"opaque fixture payload")
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "fixture.mp4"
        path.write_bytes(data)
        boxes = inspect_mp4(path)
    frames = [{"pts": n * spec.fps_den, "duration": spec.fps_den, "md5": f"{n:032x}",
               "keyframe": n % gop == 0, "decode_error_flags": 0, "flags": 0,
               "type": "I" if n % gop == 0 else "B" if spec.requested_b_frames and n % 3 == 1 else "P"}
              for n in range(spec.frame_count)]
    boundaries = [{"requested_pts": frame["pts"], "frame_count": len(frames) - n,
                   "frames": copy.deepcopy(frames[n:])}
                  for n, frame in enumerate(frames) if frame["keyframe"]]
    video_packets = [{"codec_type": "video", "stream_index": 0, "time_base": [1, spec.fps_num],
                      "pts": n * spec.fps_den, "dts": n * spec.fps_den - reorder, "duration": spec.fps_den}
                     for n in range(spec.frame_count)]
    audio_packets = [{"codec_type": "audio", "stream_index": 1, "time_base": [1, 48000],
                      "pts": start, "dts": start, "duration": min(1024, spec.audio_samples - start),
                      "skip": {"leading": delay, "trailing": 0} if start == -delay else None}
                     for start in range(-delay, spec.audio_samples, 1024)]
    case = {
        "boxes": boxes,
        "encoded": {"report": {"video_frames": spec.frame_count, "audio_samples": spec.audio_samples,
                               "video_eof": True, "audio_eof": True, "faststart_read_opens": 1,
                               "faststart_read_closes": 1, "output_bytes": len(data),
                               "info": {"audio_initial_padding": delay}}},
        "video": {"time_base": [1, spec.fps_num], "stream_index": 0, "stream_start_pts": 0, "frames": frames},
        "gops": {"time_base": [1, spec.fps_num], "boundary_count": len(boundaries), "boundaries": boundaries},
        "audio": {mode: {"time_base": [1, 48000], "stream_index": 1, "stream_start_pts": 0,
                         "first_sample_pts": -delay if mode == "manual" else 0}
                  for mode in ("ordinary", "manual")},
        "packets": {"packets": video_packets + audio_packets},
        "ffmpeg_checks": {"passed": True, "observations": {
            mode: {"event_timing_qualified": True} for mode in ("ordinary", "manual")}},
        "avfoundation": {"checks": {"passed": True, "outcome": "passed", "event_timing_qualified": True},
                         "observation": {"track": {"time_range": {
                             "start": {"value": 0, "timescale": 48000, "flags": 1, "epoch": 0}}}}},
    }
    return spec, case, len(data)


def replace_at(value, path, replacement):
    for key in path[:-1]:
        value = value[key]
    value[path[-1]] = replacement


class NativeEncodeOracleTests(unittest.TestCase):
    def test_nonzero_range_retains_original_sample_phase(self):
        self.assertEqual(RangeCaseSpec(frame_count=1).audio_samples, 1602)
        self.assertEqual(RangeCaseSpec(frame_count=1, project_start=1).audio_samples, 1601)

    def test_short_audio_requires_all_three_absolute_events(self):
        spec = RangeCaseSpec(frame_count=1, fps_num=60, fps_den=1)
        for shift, missing, accepted in ((0, False, True), (1024, False, False), (0, True, False)):
            pcm = [0.0] * 4096
            for event in (100, 400, 600):
                if missing and event == 100:
                    continue
                pcm[(event + shift) * 2] = 0.75
                pcm[(event + shift) * 2 + 1] = -0.65
            result = inspect_pcm_events(spec, pcm, [PcmSpan(0, 2048, 0)],
                                        tolerance_samples=799, search_radius=64)
            self.assertEqual(result["passed"], accepted)
            self.assertEqual(result["observations"]["event_timing_qualified"], accepted)

    def fixture(self):
        frames = [{"pts": n * 1001, "duration": 1001, "md5": str(n), "keyframe": n % 2 == 0,
                   "decode_error_flags": 0, "flags": 0} for n in range(4)]
        video = {"time_base": [1, 30000], "frames": frames}
        fresh = {"time_base": [1, 30000], "boundary_count": 2,
                 "boundaries": [{"requested_pts": frames[n]["pts"], "frames": copy.deepcopy(frames[n:]),
                                 "frame_count": 4 - n} for n in (0, 2)]}
        return video, fresh

    def test_fresh_suffix_requires_every_pixel_hash_and_clock(self):
        video, fresh = self.fixture()
        self.assertTrue(all(row["passed"] for row in inspect_gops(video, fresh)))
        for field, value in (("md5", "different"), ("pts", 2003), ("duration", 1000),
                             ("flags", 1), ("decode_error_flags", 1)):
            with self.subTest(field=field):
                changed = copy.deepcopy(fresh)
                changed["boundaries"][1]["frames"][0][field] = value
                self.assertFalse(all(row["passed"] for row in inspect_gops(video, changed)))

    def test_missing_boundary_and_late_decoder_recovery_fail(self):
        video, fresh = self.fixture()
        changed = copy.deepcopy(fresh)
        changed["boundaries"].pop()
        changed["boundary_count"] = 1
        self.assertFalse(all(row["passed"] for row in inspect_gops(video, changed)))
        changed = copy.deepcopy(fresh)
        changed["boundaries"][1]["frames"].pop(0)
        changed["boundaries"][1]["frame_count"] -= 1
        self.assertFalse(all(row["passed"] for row in inspect_gops(video, changed)))

    def test_complete_acceptance_uses_exact_audio_and_video_edit_clocks(self):
        for spec in (RangeCaseSpec(frame_count=90, requested_b_frames=2),
                     RangeCaseSpec(frame_count=1, fps_num=60, fps_den=1),
                     RangeCaseSpec(frame_count=1, project_start=1)):
            with self.subTest(spec=spec):
                spec, case, size = boundary_fixture(spec)
                result = inspect_native_case(spec, case, size)
                self.assertTrue(result["passed"], result)
                if spec.project_start == 1:
                    edits = case["boxes"]["edit_lists"]
                    self.assertEqual(edits[0]["entries"][0]["segment_duration"], 8005)
                    self.assertEqual(edits[1]["entries"][0]["segment_duration"], 8008)

    def test_unqualified_or_failed_reader_cannot_pass_boundary_acceptance(self):
        spec, original, size = boundary_fixture()
        mutations = [
            (("avfoundation", "checks", "outcome"), "unqualified"),
            (("avfoundation", "checks", "event_timing_qualified"), False),
            (("avfoundation", "checks", "passed"), False),
            (("ffmpeg_checks", "passed"), False),
            (("ffmpeg_checks", "observations", "ordinary", "event_timing_qualified"), False),
            (("ffmpeg_checks", "observations", "manual", "event_timing_qualified"), False),
        ]
        for path, value in mutations:
            with self.subTest(path=path):
                changed = copy.deepcopy(original)
                replace_at(changed, path, value)
                self.assertFalse(inspect_native_case(spec, changed, size)["passed"])

    def test_missing_or_overlong_gops_and_zero_actual_b_pictures_fail(self):
        spec, original, size = boundary_fixture()
        changed = copy.deepcopy(original)
        changed["gops"]["boundaries"].pop()
        changed["gops"]["boundary_count"] -= 1
        self.assertFalse(inspect_native_case(spec, changed, size)["passed"])
        changed = copy.deepcopy(original)
        for frame in changed["video"]["frames"][1:]:
            frame["keyframe"] = False
        changed["gops"]["boundaries"] = changed["gops"]["boundaries"][:1]
        changed["gops"]["boundary_count"] = 1
        result = inspect_native_case(spec, changed, size)
        self.assertFalse(result["passed"])
        self.assertTrue(all(row["passed"] for row in inspect_gops(changed["video"], changed["gops"])))
        self.assertFalse(next(row for row in result["checks"] if row["label"].startswith("observed GOP"))["passed"])
        changed = copy.deepcopy(original)
        for frame in changed["video"]["frames"]:
            if frame["type"] == "B":
                frame["type"] = "P"
        self.assertFalse(inspect_native_case(spec, changed, size)["passed"])

    def test_nonzero_stream_starts_and_inexact_movie_clock_fail(self):
        spec, original, size = boundary_fixture()
        paths = [("video", "stream_start_pts"), ("audio", "ordinary", "stream_start_pts"),
                 ("audio", "manual", "stream_start_pts"),
                 ("avfoundation", "observation", "track", "time_range", "start", "value"),
                 ("boxes", "timescales", 0, "timescale"), ("boxes", "timescales", 0, "duration_ticks")]
        for path in paths:
            with self.subTest(path=path):
                changed = copy.deepcopy(original)
                replace_at(changed, path, 1)
                self.assertFalse(inspect_native_case(spec, changed, size)["passed"])

    def test_edit_lists_reject_gaps_rates_wrong_offsets_and_duration_changes(self):
        spec, original, size = boundary_fixture()
        for track in (0, 1):
            for field, value in (("media_time", -1), ("media_time", -2), ("media_time", 7),
                                 ("media_rate_integer", 0), ("media_rate_integer", 2),
                                 ("media_rate_fraction", 1), ("segment_duration", 0),
                                 ("segment_duration", 720719)):
                with self.subTest(track=track, field=field, value=value):
                    changed = copy.deepcopy(original)
                    changed["boxes"]["edit_lists"][track]["entries"][0][field] = value
                    self.assertFalse(inspect_native_case(spec, changed, size)["passed"])
        for malformed in ([], [{"media_time": 0}], original["boxes"]["edit_lists"][0]["entries"] * 2):
            with self.subTest(entries=malformed):
                changed = copy.deepcopy(original)
                changed["boxes"]["edit_lists"][0]["entries"] = malformed
                changed["boxes"]["edit_lists"][0]["entry_count"] = len(malformed)
                self.assertFalse(inspect_native_case(spec, changed, size)["passed"])
        changed = copy.deepcopy(original)
        changed["boxes"]["edit_lists"].pop()
        self.assertFalse(inspect_native_case(spec, changed, size)["passed"])

    def test_priming_and_track_binding_require_agreement_with_measured_clocks(self):
        spec, original, size = boundary_fixture()
        mutations = [
            (("encoded", "report", "info", "audio_initial_padding"), 1025),
            (("audio", "manual", "first_sample_pts"), 0),
            (("audio", "ordinary", "first_sample_pts"), 1024),
            (("boxes", "handlers", 0, "handler_type"), "vide"),
            (("boxes", "timescales", 1, "timescale"), 44100),
            (("packets", "packets", 0, "dts"), 0),
        ]
        for path, value in mutations:
            with self.subTest(path=path):
                changed = copy.deepcopy(original)
                replace_at(changed, path, value)
                self.assertFalse(inspect_native_case(spec, changed, size)["passed"])
        changed = copy.deepcopy(original)
        changed["boxes"]["edit_lists"][0]["parent_offset"] = changed["boxes"]["edit_lists"][1]["parent_offset"]
        self.assertFalse(inspect_native_case(spec, changed, size)["passed"])


if __name__ == "__main__":
    unittest.main()
