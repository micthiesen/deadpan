"""Pure native adapter tests. No file/media/process access."""

from array import array
from collections.abc import Sequence
from copy import deepcopy
from fractions import Fraction
import unittest
from unittest import mock

from encoder_oracle import CaseSpec
from native_audio_oracle import MAX_SAMPLES, OracleError, UnqualifiedTiming, exact_time, inspect_native_audio


SPEC = CaseSpec(frame_count=120, fps_num=60, fps_den=1)


def time(value, scale=48000, flags=1, epoch=0):
    return {"value": value, "timescale": scale, "flags": flags, "epoch": epoch}


def fixture(*, shift=0):
    count = 96000
    pcm = array("f", [0.0]) * (count * 2)
    for sample in (100, 48000, 95800):
        pcm[sample * 2] = 0.75
        pcm[sample * 2 + 1] = -0.65
    asbd = {"sample_rate": 48000.0, "format_id": 0x6C70636D, "format_flags": 9,
            "bytes_per_packet": 8, "frames_per_packet": 1, "bytes_per_frame": 8,
            "channels_per_frame": 2, "bits_per_channel": 32}
    buffer = {"index": 0, "num_samples": count, "data_ready": True,
              "pts": time(shift), "dts": time(0, 0, 0), "duration": time(count),
              "output_pts": time(shift), "output_duration": time(count),
              "timing": [{"pts": time(shift), "dts": time(0, 0, 0), "duration": time(1)}],
              "asbd": asbd,
              "attachments": {key: None for key in ("trim_start", "trim_end", "speed", "reverse",
                                                     "empty_media", "reset_decoder", "drain_after_decoding")},
              "sample_attachment_count": 0, "sample_attachments": [],
              "sample_offset": 0, "byte_count": count * 8}
    stored = deepcopy(buffer)
    stored.pop("sample_offset")
    stored.pop("byte_count")
    stored["num_samples"] = 94  # Encoded AAC packets, deliberately not PCM frames.
    stored["attachments"]["trim_start"] = time(1024)
    stored["asbd"]["format_id"] = 0x61616320
    observation = {
        "schema_version": 1, "kind": "avfoundation_audio", "status": "completed",
        "input_sha256": "0" * 64, "final_input_sha256": "0" * 64,
        "asset_duration": time(2, 1), "provides_precise_duration_and_timing": True,
        "track": {"id": 1, "time_range": {"start": time(0), "duration": time(count)},
                  "source_formats": [deepcopy(stored["asbd"])], "segments": []},
        "stored": {"status": "completed", "reader_status": 2, "buffer_count": 1, "buffers": [stored]},
        "pcm": {"status": "completed", "reader_status": 2, "buffer_count": 1, "buffers": [buffer],
                "total_samples": count, "pcm_bytes": count * 8,
                "pcm_format": "f32le_interleaved_stereo_48000"},
    }
    return observation, pcm


class NativeAudioOracleTests(unittest.TestCase):
    def test_exact_cmtime_is_rational_and_never_rounds(self):
        self.assertEqual(exact_time(time(1001, 30000)), Fraction(1001, 30000))
        self.assertEqual(exact_time(time(-1024)), Fraction(-8, 375))
        for value in (time(1, 48000, 3), time(1, 0, 0), time(0, 48000, 5),
                      time(0, 48000, 9), time(0, 48000, 17), time(0, 48000, 1, 1)):
            with self.subTest(value=value), self.assertRaises(UnqualifiedTiming):
                exact_time(value)
        with self.assertRaises(OracleError):
            exact_time(time(1 << 63))

    def test_unmodified_absolute_native_pcm_reuses_existing_event_oracle(self):
        observation, pcm = fixture()
        before = deepcopy(observation)
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "passed", result)
        self.assertTrue(result["event_timing_qualified"])
        self.assertEqual([event["error_samples"] for event in result["observations"]["native"]["events"]], [0] * 6)
        self.assertEqual(result["observations"]["native_mapping"]["pcm_spans"], 1)
        self.assertEqual(observation, before)
        self.assertIs(result["observations"]["raw_native_observation"], observation)
        self.assertNotIn("frame_skip_metadata", result["observations"]["native"])
        self.assertNotIn("discarded_frames", result["observations"]["native"])
        self.assertFalse(any("no decode errors" in check["label"] for check in result["checks"]))
        self.assertTrue(any(check["label"] == "native: both readers reached completed status"
                            for check in result["checks"]))
        self.assertEqual(result["observations"]["raw_native_observation"]["stored"]["buffers"][0]["num_samples"], 94)

    def test_native_observations_do_not_use_the_ffmpeg_decoder_adapter(self):
        observation, pcm = fixture()
        with mock.patch("encoder_oracle._audio_checks", side_effect=AssertionError("FFmpeg adapter must not be used")):
            result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "passed", result)
        labels = [check["label"] for check in result["checks"]]
        self.assertNotIn("native: explicit decoder mode", labels)
        self.assertNotIn("native: first-sample summary agrees with raw PTS", labels)
        self.assertFalse(any("decode errors" in label for label in labels))

    def test_1024_sample_shift_fails_60fps_without_event_alignment(self):
        observation, pcm = fixture(shift=1024)
        for tolerance in (799, 800):
            with self.subTest(tolerance=tolerance):
                result = inspect_native_audio(SPEC, observation, pcm, tolerance_samples=tolerance)
                self.assertEqual(result["outcome"], "failed")
                self.assertFalse(result["event_timing_qualified"])
                self.assertEqual([event["error_samples"] for event in result["observations"]["native"]["events"]], [1024] * 6)
                self.assertFalse(result["observations"]["native_mapping"]["event_alignment_applied"])

    def test_pcm_nonzero_trim_is_unqualified_without_double_trim(self):
        observation, pcm = fixture()
        first = observation["pcm"]["buffers"][0]
        first["attachments"]["trim_start"] = time(1024)
        first["output_pts"] = time(1024)
        first["output_duration"] = time(96000 - 1024)
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "unqualified")
        self.assertTrue(result["passed"])
        self.assertFalse(result["event_timing_qualified"])
        self.assertNotIn("events", result["observations"]["native"])
        self.assertTrue(any("second trim" in item for item in result["unqualified"]))
        self.assertEqual(len(pcm), 192000)
        self.assertEqual(pcm[200], 0.75)

    def test_output_pts_difference_cannot_become_a_second_origin_shift(self):
        observation, pcm = fixture()
        observation["pcm"]["buffers"][0]["output_pts"] = time(1024)
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "unqualified")
        self.assertFalse(result["event_timing_qualified"])
        self.assertEqual(result["observations"]["native_buffer_timing"][0]["pts_samples"], 0)
        self.assertEqual(result["observations"]["native_buffer_timing"][0]["output_pts_samples"], 1024)

    def test_zero_attachments_and_equivalent_rational_clocks_are_not_false_ambiguity(self):
        observation, pcm = fixture()
        first = observation["pcm"]["buffers"][0]
        first["attachments"].update(trim_start=time(0, 600), trim_end=time(0), speed=1.0, reverse=False)
        first["duration"] = time(2, 1)
        first["output_duration"] = time(1200, 600)
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "passed", result)

    def test_bad_asbd_failed_reader_and_nonfinite_pcm_cannot_qualify(self):
        for mutation in ("noninterleaved", "bigendian", "unready", "reader", "nan", "hash"):
            with self.subTest(mutation=mutation):
                observation, pcm = fixture()
                first = observation["pcm"]["buffers"][0]
                if mutation == "noninterleaved":
                    first["asbd"]["format_flags"] |= 32
                elif mutation == "bigendian":
                    first["asbd"]["format_flags"] |= 2
                elif mutation == "unready":
                    first["data_ready"] = False
                elif mutation == "reader":
                    observation["pcm"]["reader_status"] = 1
                elif mutation == "nan":
                    pcm[20000] = float("nan")
                else:
                    observation["final_input_sha256"] = "1" * 64
                result = inspect_native_audio(SPEC, observation, pcm)
                self.assertEqual(result["outcome"], "failed")
                self.assertFalse(result["event_timing_qualified"])

    def test_rounded_and_fractional_sample_times_stay_unqualified(self):
        for value in (time(0, 48000, 3), time(1, 96000), time(0, 48000, 1, 2)):
            with self.subTest(value=value):
                observation, pcm = fixture()
                observation["pcm"]["buffers"][0]["pts"] = value
                result = inspect_native_audio(SPEC, observation, pcm)
                self.assertEqual(result["outcome"], "unqualified")
                self.assertFalse(result["event_timing_qualified"])

    def test_per_sample_timing_is_not_confused_with_whole_buffer_duration(self):
        observation, pcm = fixture()
        observation["pcm"]["buffers"][0]["timing"][0]["duration"] = time(96000)
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "unqualified")
        self.assertTrue(any("one 48000 Hz sample" in item for item in result["unqualified"]))

    def test_contiguous_bytes_do_not_hide_a_timestamp_gap(self):
        observation, pcm = fixture()
        first = observation["pcm"]["buffers"][0]
        second = deepcopy(first)
        first.update(num_samples=48000, duration=time(48000), output_duration=time(48000), byte_count=48000 * 8)
        second.update(index=1, num_samples=48000, sample_offset=48000, byte_count=48000 * 8,
                      pts=time(48001), output_pts=time(48001), duration=time(48000), output_duration=time(48000))
        second["timing"][0]["pts"] = time(48001)
        observation["pcm"].update(buffer_count=2, buffers=[first, second])
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "failed")
        self.assertFalse(result["event_timing_qualified"])

    def test_zero_sample_marker_is_not_eof_and_do_not_display_is_unqualified(self):
        observation, pcm = fixture()
        marker = deepcopy(observation["pcm"]["buffers"][0])
        marker.update(index=0, num_samples=0, duration=time(0), output_duration=time(0),
                      timing=[], asbd=None, byte_count=0)
        observation["pcm"]["buffers"][0]["index"] = 1
        observation["pcm"]["buffers"].insert(0, marker)
        observation["pcm"]["buffer_count"] = 2
        self.assertEqual(inspect_native_audio(SPEC, observation, pcm)["outcome"], "passed")
        first = observation["pcm"]["buffers"][1]
        first["sample_attachment_count"] = 1
        first["sample_attachments"] = [{"sample_index": 0, "do_not_display": True}]
        result = inspect_native_audio(SPEC, observation, pcm)
        self.assertEqual(result["outcome"], "unqualified")

    def test_oversized_pcm_is_rejected_before_access(self):
        class Oversized(Sequence):
            def __len__(self):
                return MAX_SAMPLES * 2 + 2

            def __getitem__(self, index):
                raise AssertionError("oversized PCM must not be inspected")

        observation, _ = fixture()
        result = inspect_native_audio(SPEC, observation, Oversized())
        self.assertEqual(result["outcome"], "failed")
        self.assertFalse(result["event_timing_qualified"])


if __name__ == "__main__":
    unittest.main()
