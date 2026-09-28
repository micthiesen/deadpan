"""Pure bounded oracle fixtures; no encoder, subprocess, media file or GPU."""

from array import array
from collections.abc import Sequence
import unittest

from encoder_oracle import (
    CaseSpec, I64_MAX, MAX_AUDIO_FRAMES, MAX_DECODED_SAMPLES, OracleError, PcmSpan, audio_boundary,
    bt709_oetf, color_reference, expected_manifest, inspect_case, movie_timescale,
    point_ceil_boundary, inspect_pcm_events,
)


COLORS = [[16, 128, 128], [235, 128, 128], [63, 102, 240],
          [173, 42, 26], [32, 240, 118]]
NEUTRALS = [[17, 128, 128], [26, 128, 128], [34, 128, 128],
            [106, 128, 128], [171, 128, 128]]
METADATA = {"color_range": "tv", "color_space": "bt709", "color_transfer": "bt709",
            "color_primaries": "bt709", "chroma_location": "left"}


def audio_fixture(spec, mode, *, origin=0, priming=0, tail=0):
    count = spec.audio_samples + priming + tail
    pcm = array("f", [0.0]) * (count * 2)
    for sample in (100, min(48000, spec.audio_samples // 2), spec.audio_samples - 200):
        pcm[(sample + priming) * 2] = 0.75
        pcm[(sample + priming) * 2 + 1] = -0.65
    frames = []
    for offset in range(0, count, 1024):
        samples = min(1024, count - offset)
        frames.append({"pts": origin + offset, "best_effort_pts": origin + offset,
                       "pkt_dts": None, "duration": samples, "nb_samples": samples,
                       "sample_offset": offset, "sample_rate": 48000, "channels": 2,
                       "channel_layout": "stereo", "skip": None, "discard": False,
                       "decode_error_flags": 0, "flags": 0})
    return ({"mode": mode, "time_base": [1, 48000], "stream_start_pts": origin,
             "stream_duration": spec.audio_samples, "initial_padding": 0,
             "trailing_padding": 0, "seek_preroll": 0, "first_sample_pts": origin,
             "decoded_samples": count, "decoded_frames": len(frames),
             "sample_rate": 48000, "channels": 2, "channel_layout": "stereo",
             "frames": frames}, pcm)


def fixture(spec=None):
    spec = spec or CaseSpec(frame_count=32, fps_num=25, fps_den=1)
    source = expected_manifest(spec)
    source.update({"color_yuv_patches": [row[:] for row in COLORS],
                   "neutral_yuv_patches": [row[:] for row in NEUTRALS],
                   "neutral_linear_levels": ["0.001", "0.01", "0.018", "0.18", "0.5"]})
    frames = []
    for number in range(spec.frame_count):
        frames.append({"pts": number * spec.fps_den,
                       "best_effort_pts": number * spec.fps_den, "duration": spec.fps_den,
                       "authored_identity": number, "keyframe": number % 15 == 0,
                       "type": "I" if number % 15 == 0 else "P", "decode_error_flags": 0,
                       "flags": 0, "sample_aspect_ratio": [1, 1], "interlaced": False,
                       **METADATA})
    video = {"time_base": [1, spec.fps_num], "frames": frames,
             "frame_count": spec.frame_count,
             "first_frame_yuv_patches": [row[:] for row in COLORS],
             "first_frame_neutral_patches": [row[:] for row in NEUTRALS]}
    probe = {"streams": [
        {"codec_type": "video", "codec_name": "h264", "profile": "High",
         "pix_fmt": "yuv420p", "field_order": "progressive", "width": spec.width,
         "height": spec.height, "sample_aspect_ratio": "1:1", **METADATA},
        {"codec_type": "audio", "codec_name": "aac", "profile": "LC",
         "channel_layout": "stereo", "sample_rate": "48000", "channels": 2},
    ]}
    packets = {"packets": [{"codec_type": "video", "time_base": [1, spec.fps_num],
                            "pts": n * spec.fps_den, "dts": n * spec.fps_den}
                           for n in range(spec.frame_count)]}
    ordinary, ordinary_pcm = audio_fixture(spec, "ordinary")
    manual, manual_pcm = audio_fixture(spec, "manual")
    return {"spec": spec, "source": source, "video": video, "probe": probe,
            "packets": packets, "ordinary_audio": ordinary, "manual_audio": manual,
            "ordinary_pcm": ordinary_pcm, "manual_pcm": manual_pcm}


def check(result, label):
    matches = [item for item in result["checks"] if item["label"] == label]
    if len(matches) != 1:
        raise AssertionError(f"expected exactly one check {label!r}: {matches!r}")
    return matches[0]


class ExactBoundaryTests(unittest.TestCase):
    def test_signed_half_sample_boundaries_use_ties_to_even(self):
        self.assertEqual([audio_boundary(n, 96000, 1) for n in range(-7, 8)],
                         [-4, -3, -2, -2, -2, -1, 0, 0, 0, 1, 2, 2, 2, 3, 4])

    def test_origin_boundary_is_not_accumulated_rounding_or_point_ceil(self):
        self.assertEqual(audio_boundary(1, 30000, 1001), 1602)
        self.assertEqual(audio_boundary(2, 30000, 1001), 3203)
        self.assertEqual(point_ceil_boundary(2, 30000, 1001), 3204)
        self.assertEqual(audio_boundary(120, 30000, 1001), 192192)
        self.assertNotEqual(120 * audio_boundary(1, 30000, 1001), 192192)
        self.assertEqual(point_ceil_boundary(-1, 96000, 1), 0)

    def test_reduced_rate_and_matrix_endpoints(self):
        self.assertEqual(movie_timescale(60000, 2002), 240000)
        self.assertEqual(CaseSpec(fps_num=60000, fps_den=2002).fps_num, 30000)
        for frames, rate, samples in ((1, 60, 800), (120, 60, 96000), (32, 25, 61440)):
            with self.subTest(frames=frames, rate=rate):
                self.assertEqual(CaseSpec(frame_count=frames, fps_num=rate, fps_den=1).audio_samples, samples)

    def test_bounded_inputs_and_checked_output(self):
        for arguments in ((True, 60, 1), (0, 0, 1), (0, 60, 0), (0, 1 << 32, 1),
                          (I64_MAX, 1, 1), (I64_MAX + 1, 60, 1)):
            with self.subTest(arguments=arguments), self.assertRaises(OracleError):
                audio_boundary(*arguments)
        for arguments in ({"frame_count": 241}, {"fps_num": 61, "fps_den": 1},
                          {"frame_count": 240, "fps_num": 1, "fps_den": 1},
                          {"width": 321}, {"requested_b_frames": True}):
            with self.subTest(arguments=arguments), self.assertRaises(OracleError):
                CaseSpec(**arguments)
        with self.assertRaises(OracleError):
            movie_timescale(4294967291, 1)


class ColorOracleTests(unittest.TestCase):
    def test_independent_known_matrix_and_transfer_codes(self):
        reference = color_reference()
        self.assertEqual(reference["color_yuv_patches"], COLORS)
        self.assertEqual(reference["neutral_yuv_patches"], NEUTRALS)
        self.assertEqual(bt709_oetf(0), 0)
        self.assertAlmostEqual(bt709_oetf(1), 1)
        self.assertAlmostEqual(bt709_oetf(0.01), 0.045)
        self.assertGreater(bt709_oetf(0.018), 0.081)
        for value in (float("nan"), float("inf"), -0.01, 1.01, True, 10 ** 1000):
            with self.subTest(value=repr(value)[:20]), self.assertRaises(OracleError):
                bt709_oetf(value)

    def test_correct_pixels_cannot_hide_wrong_transfer_or_range(self):
        for field, wrong in (("color_transfer", "iec61966-2-1"), ("color_range", "pc")):
            with self.subTest(field=field):
                inputs = fixture()
                inputs["probe"]["streams"][0][field] = wrong
                inputs["video"]["frames"][10][field] = wrong
                result = inspect_case(**inputs)
                self.assertFalse(result["passed"])
                self.assertFalse(check(result, f"metadata: video {field}")["passed"])
                self.assertFalse(check(result, "video: decoded color interpretation")["passed"])
                self.assertTrue(check(result, "color: exact pre-encode neutral_yuv_patches")["passed"])

    def test_correct_tags_do_not_hide_wrong_oetf_or_missing_patches(self):
        inputs = fixture()
        inputs["source"]["neutral_yuv_patches"][3][0] = 116
        result = inspect_case(**inputs)
        self.assertFalse(check(result, "color: exact pre-encode neutral_yuv_patches")["passed"])
        inputs["video"]["first_frame_yuv_patches"].pop()
        result = inspect_case(**inputs)
        self.assertFalse(check(result, "color: valid bounded observations")["passed"])
        self.assertTrue(check(result, "video: every visible authored identity")["passed"])


class PcmEventTests(unittest.TestCase):
    def test_absolute_events_are_independent_of_decoder_buffer_partition(self):
        inputs = fixture()
        spec, pcm = inputs["spec"], inputs["ordinary_pcm"]
        whole = inspect_pcm_events(spec, pcm, [PcmSpan(0, spec.audio_samples, 0)])
        boundaries = [0, 101, 8191, 30721, 48000, spec.audio_samples]
        spans = [PcmSpan(left, right - left, left) for left, right in zip(boundaries, boundaries[1:])]
        self.assertEqual(inspect_pcm_events(spec, pcm, spans), whole)
        self.assertTrue(whole["passed"])
        self.assertNotIn("frame_skip_metadata", whole["observations"])
        self.assertNotIn("discarded_frames", whole["observations"])

    def test_encoder_retains_public_event_labels_values_and_diagnostics(self):
        inputs = fixture()
        spec = inputs["spec"]
        native_clock = inspect_pcm_events(spec, inputs["ordinary_pcm"],
                                         [PcmSpan(0, spec.audio_samples, 0)], label="ordinary")
        encoded = inspect_case(**inputs)
        self.assertEqual(encoded["observations"]["ordinary"]["events"], native_clock["observations"]["events"])
        labels = {entry["label"] for entry in native_clock["checks"]}
        self.assertEqual([entry for entry in encoded["checks"] if entry["label"] in labels], native_clock["checks"])

    def test_invalid_span_layouts_and_nonintegral_clocks_reject(self):
        inputs = fixture()
        spec, pcm = inputs["spec"], inputs["ordinary_pcm"]
        for spans in ([PcmSpan(1, spec.audio_samples - 1, 0)],
                      [PcmSpan(0, 100, 0), PcmSpan(100, spec.audio_samples - 100, 101)],
                      [PcmSpan(0, 100, 0), PcmSpan(99, spec.audio_samples - 100, 100)],
                      [PcmSpan(0, spec.audio_samples - 1, 0)]):
            with self.subTest(spans=spans), self.assertRaises(OracleError):
                inspect_pcm_events(spec, pcm, spans)
        for start in (0.5, True, I64_MAX):
            with self.subTest(start=start), self.assertRaises(OracleError):
                PcmSpan(0, 1, start)

    def test_record_cap_preflights_pcm_and_all_pcm_is_finite(self):
        class Untouched(Sequence):
            def __len__(self):
                raise AssertionError("PCM must not be accessed after an oversized span inventory")

            def __getitem__(self, index):
                raise AssertionError("PCM must not be accessed after an oversized span inventory")

        with self.assertRaises(OracleError):
            inspect_pcm_events(CaseSpec(), Untouched(), [PcmSpan(0, 1, 0)] * (MAX_AUDIO_FRAMES + 1))
        inputs = fixture()
        inputs["ordinary_pcm"][20000] = float("nan")
        with self.assertRaises(OracleError):
            inspect_pcm_events(inputs["spec"], inputs["ordinary_pcm"], [PcmSpan(0, inputs["spec"].audio_samples, 0)])


class EncodedObservationTests(unittest.TestCase):
    def test_exact_synthetic_observations_pass_only_the_declared_scope(self):
        result = inspect_case(**fixture())
        self.assertTrue(result["passed"], result["checks"])
        self.assertTrue(result["observations"]["ordinary"]["event_timing_qualified"])
        self.assertTrue(result["observations"]["manual"]["exact_event_samples"])
        self.assertFalse(result["observations"]["video"]["closed_gop_qualified"])
        self.assertIn("second decoder implementation", result["unqualified"])
        self.assertFalse(result["observations"]["ordinary"]["alignment_or_event_based_cropping_applied"])

    def test_shifted_aac_keeps_signed_absolute_errors_without_realignment(self):
        for displacement in (-1024, 1024):
            with self.subTest(displacement=displacement):
                inputs = fixture()
                inputs["ordinary_audio"], inputs["ordinary_pcm"] = audio_fixture(
                    inputs["spec"], "ordinary", origin=displacement)
                result = inspect_case(**inputs)
                observed = result["observations"]["ordinary"]
                self.assertFalse(result["passed"])
                self.assertEqual([event["error_samples"] for event in observed["events"]], [displacement] * 6)
                self.assertEqual(observed["stream_end_error_samples"], displacement)
                self.assertTrue(result["observations"]["manual"]["exact_event_samples"])

    def test_declared_subframe_tolerance_keeps_exact_sample_failures_visible(self):
        inputs = fixture(CaseSpec())
        inputs["ordinary_audio"], inputs["ordinary_pcm"] = audio_fixture(inputs["spec"], "ordinary", origin=1024)
        result = inspect_case(**inputs, tolerance_samples=1601)
        self.assertTrue(result["passed"], result["checks"])
        self.assertFalse(result["observations"]["ordinary"]["exact_event_samples"])
        exact = check(result, "ordinary: left event 100 exact")
        self.assertTrue(exact["diagnostic"])
        self.assertFalse(exact["passed"])
        self.assertEqual(exact["details"]["error_samples"], 1024)
        self.assertEqual(exact["details"]["error_seconds"], "8/375")

    def test_sixty_fps_rejects_1024_delay_and_does_not_hide_errors_at_invalid_tolerance(self):
        inputs = fixture(CaseSpec(frame_count=120, fps_num=60, fps_den=1))
        inputs["ordinary_audio"], inputs["ordinary_pcm"] = audio_fixture(inputs["spec"], "ordinary", origin=1024)
        for tolerance in (799, 800):
            with self.subTest(tolerance=tolerance):
                result = inspect_case(**inputs, tolerance_samples=tolerance)
                self.assertFalse(result["passed"])
                self.assertEqual([event["error_samples"] for event in result["observations"]["ordinary"]["events"]], [1024] * 6)
                self.assertEqual(check(result, "timing: declared encoded tolerance strictly below one frame")["passed"], tolerance < 800)

    def test_priming_and_tail_keep_raw_physical_boundaries_and_skip_metadata(self):
        inputs = fixture()
        manual, pcm = audio_fixture(inputs["spec"], "manual", origin=-1024, priming=1024, tail=320)
        manual["stream_duration"] += 1024
        manual["initial_padding"] = 1024
        manual["frames"][0]["skip"] = {"leading": 1024, "trailing": 0,
                                      "leading_reason": 0, "trailing_reason": 0}
        manual["frames"][0]["discard"] = True
        inputs["manual_audio"], inputs["manual_pcm"] = manual, pcm
        result = inspect_case(**inputs)
        self.assertTrue(result["passed"], result["checks"])
        observed = result["observations"]["manual"]
        self.assertEqual(observed["physical_start_sample"], -1024)
        self.assertEqual(observed["physical_tail_after_authored_end"], 320)
        self.assertEqual(observed["discarded_frames"], [0])
        self.assertEqual(observed["frame_skip_metadata"][0]["leading"], 1024)
        self.assertTrue(observed["exact_event_samples"])
        self.assertFalse(observed["edge_content_qualified"])

    def test_short_overlapping_markers_are_explicitly_unqualified_not_codec_failure(self):
        result = inspect_case(**fixture(CaseSpec(frame_count=1, fps_num=60, fps_den=1, pcm_kind="edges")))
        self.assertTrue(result["passed"], result["checks"])
        observed = result["observations"]["ordinary"]
        self.assertFalse(observed["event_timing_qualified"])
        self.assertEqual([event["actual_peak_sample"] for event in observed["events"]], [100] * 6)
        self.assertIn("ordinary event timing: authored marker diagnostic windows overlap", result["unqualified"])
        distinct = check(result, "ordinary: left events select distinct measured peaks")
        self.assertFalse(distinct["passed"])
        self.assertTrue(distinct["diagnostic"])

    def test_swapped_duplicate_and_missing_frames_are_not_hidden_by_valid_pts(self):
        for change in ("swap", "duplicate", "missing"):
            with self.subTest(change=change):
                inputs = fixture()
                frames = inputs["video"]["frames"]
                if change == "swap":
                    frames[10]["authored_identity"], frames[11]["authored_identity"] = 11, 10
                elif change == "duplicate":
                    frames[11]["authored_identity"] = 10
                else:
                    frames.pop(11)
                result = inspect_case(**inputs)
                self.assertFalse(result["passed"])
                self.assertFalse(check(result, "video: every visible authored identity")["passed"])

    def test_terminal_duration_is_observed_not_invented_from_count(self):
        inputs = fixture()
        inputs["video"]["frames"][-1]["duration"] = 0
        result = inspect_case(**inputs)
        self.assertFalse(check(result, "video: exact terminal boundary")["passed"])
        self.assertFalse(check(result, "video: exact per-frame durations")["passed"])
        self.assertTrue(check(result, "video: exact frame count")["passed"])

    def test_unspecified_raw_sar_uses_only_explicit_positive_stream_evidence(self):
        inputs = fixture()
        video = inputs["video"]
        video["stream_sample_aspect_ratio"] = [1, 1]
        video["codec_parameters_sample_aspect_ratio"] = [0, 1]
        for frame in video["frames"]:
            frame["sample_aspect_ratio"] = [0, 1]
        result = inspect_case(**inputs)
        self.assertTrue(result["passed"], result["checks"])
        aspects = result["observations"]["video"]["frame_sample_aspect_ratios"]
        self.assertTrue(all(value["raw"] == [0, 1] and value["effective"] == "1"
                            and value["source"] == "stream" for value in aspects))
        self.assertEqual(result["observations"]["video"]["codec_parameters_sample_aspect_ratio"], [0, 1])

    def test_unknown_sar_without_stream_evidence_fails_but_retains_frame_timing(self):
        inputs = fixture()
        video = inputs["video"]
        video["codec_parameters_sample_aspect_ratio"] = [1, 1]
        video["frames"][0]["sample_aspect_ratio"] = [0, 1]
        result = inspect_case(**inputs)
        self.assertFalse(result["passed"])
        self.assertFalse(check(result, "video: progressive square-pixel frames")["passed"])
        self.assertTrue(check(result, "video: exact rational CFR PTS")["passed"])
        self.assertTrue(check(result, "video: every visible authored identity")["passed"])
        self.assertIsNone(result["observations"]["video"]["frame_sample_aspect_ratios"][0]["effective"])
        self.assertTrue(check(result, "metadata: square pixels")["passed"])

    def test_malformed_or_nonsquare_sar_cannot_be_replaced_with_square_pixels(self):
        cases = (([-1, 1], [1, 1]), ([1, 0], [1, 1]), ([0, -1], [1, 1]),
                 (None, [1, 1]), ([0, 1], [0, 1]), ([0, 1], [-1, 1]),
                 ([0, 1], [1, 0]), ([0, 1], [4, 3]), ([4, 3], [1, 1]))
        for raw, stream in cases:
            with self.subTest(raw=raw, stream=stream):
                inputs = fixture()
                inputs["video"]["frames"][0]["sample_aspect_ratio"] = raw
                inputs["video"]["stream_sample_aspect_ratio"] = stream
                result = inspect_case(**inputs)
                self.assertFalse(check(result, "video: progressive square-pixel frames")["passed"])
                self.assertTrue(check(result, "video: exact rational CFR PTS")["passed"])
                self.assertTrue(check(result, "video: exact terminal boundary")["passed"])

    def test_video_corrupt_flag_rejects_zero_decode_error_flags(self):
        inputs = fixture()
        inputs["video"]["frames"][10]["flags"] = 1
        result = inspect_case(**inputs)
        self.assertFalse(result["passed"])
        failure = check(result, "video: no concealed decode/timestamp errors")
        self.assertFalse(failure["passed"])
        self.assertEqual(failure["details"], [{"frame": 10, "flags": 1, "error": "corrupt frame flag"}])
        self.assertTrue(check(result, "video: every visible authored identity")["passed"])

    def test_keyframe_flag_is_not_corruption(self):
        inputs = fixture()
        inputs["video"]["frames"][0]["flags"] = 2
        for mode in ("ordinary", "manual"):
            inputs[mode + "_audio"]["frames"][0]["flags"] = 2
        self.assertTrue(inspect_case(**inputs)["passed"])

    def test_both_audio_modes_reject_corrupt_or_decode_error_despite_exact_peaks(self):
        for mode in ("ordinary", "manual"):
            for field, value in (("flags", 1), ("decode_error_flags", 1)):
                with self.subTest(mode=mode, field=field):
                    inputs = fixture()
                    inputs[f"{mode}_audio"]["frames"][10][field] = value
                    result = inspect_case(**inputs)
                    self.assertFalse(result["passed"])
                    self.assertFalse(check(result, f"{mode}: exact sample grid and no decode errors")["passed"])
                    self.assertFalse(result["observations"][mode]["event_timing_qualified"])
                    other = "manual" if mode == "ordinary" else "ordinary"
                    self.assertTrue(result["observations"][other]["exact_event_samples"])

    def test_reordered_packet_pts_are_allowed_but_nonmonotonic_dts_are_not(self):
        inputs = fixture(CaseSpec(frame_count=32, fps_num=25, fps_den=1, requested_b_frames=2))
        packets = inputs["packets"]["packets"]
        packets[1]["pts"], packets[2]["pts"] = packets[2]["pts"], packets[1]["pts"]
        inputs["video"]["frames"][1]["type"] = "B"
        result = inspect_case(**inputs)
        self.assertTrue(result["passed"], result["checks"])
        self.assertTrue(result["observations"]["packets"]["presentation_reordering_observed"])
        self.assertEqual(result["observations"]["video"]["maximum_b_run"], 1)
        packets[1]["dts"] = packets[0]["dts"]
        result = inspect_case(**inputs)
        self.assertFalse(check(result, "packets: strictly increasing video DTS")["passed"])

    def test_pcm_is_checked_outside_marker_search_and_cannot_overlap_frames(self):
        for invalid in (float("nan"), float("inf")):
            with self.subTest(invalid=invalid):
                inputs = fixture()
                inputs["ordinary_pcm"][10000 * 2] = invalid
                result = inspect_case(**inputs)
                self.assertFalse(check(result, "ordinary: valid bounded observations")["passed"])
                self.assertTrue(result["observations"]["manual"]["exact_event_samples"])
        inputs = fixture()
        inputs["ordinary_audio"]["frames"][1]["sample_offset"] = 0
        result = inspect_case(**inputs)
        self.assertFalse(check(result, "ordinary: contiguous absolute sample frames")["passed"])
        self.assertEqual(result["observations"]["ordinary"]["event_search_skipped"], "invalid absolute PCM frame layout")

    def test_missing_audio_timestamps_fail_without_hiding_video_observations(self):
        inputs = fixture()
        inputs["ordinary_audio"]["frames"][0]["pts"] = None
        result = inspect_case(**inputs)
        self.assertFalse(result["passed"])
        self.assertFalse(check(result, "ordinary: valid bounded observations")["passed"])
        self.assertTrue(check(result, "video: exact rational CFR PTS")["passed"])

    def test_pcm_size_rejects_before_access_and_malformed_fields_do_not_abort_other_sections(self):
        class Oversized(Sequence):
            def __len__(self):
                return MAX_DECODED_SAMPLES * 2 + 2

            def __getitem__(self, key):
                raise AssertionError("oversized PCM must not be accessed")

        inputs = fixture()
        inputs["ordinary_pcm"] = Oversized()
        inputs["source"]["audio_samples"] += 1
        inputs["probe"]["streams"][0]["sample_aspect_ratio"] = "0:0"
        result = inspect_case(**inputs)
        self.assertFalse(check(result, "ordinary: valid bounded observations")["passed"])
        self.assertFalse(check(result, "input: exact audio_samples")["passed"])
        self.assertFalse(check(result, "metadata: valid bounded observations")["passed"])
        self.assertTrue(result["observations"]["manual"]["event_timing_qualified"])


if __name__ == "__main__":
    unittest.main()
