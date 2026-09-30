import unittest

from encoder_oracle import COLOR_METADATA, CaseSpec, PcmSpan
from project_encode_oracle import compare_pcm, compare_plane, inspect_video


class ProjectOracleTests(unittest.TestCase):
    def test_video_rejects_conflicting_stream_or_codec_aspect(self):
        observation = {"frames": [{"pts": 0, "best_effort_pts": 0, "duration": 1001,
            "width": 320, "height": 180, "sample_aspect_ratio": [1, 1],
            "decode_error_flags": 0, "flags": 2, "interlaced": False, "keyframe": True,
            "type": "I", **COLOR_METADATA}], "frame_count": 1, "decoder_drained": True,
            "profile": "High", "stream_start_pts": 0, "stream_duration": 1001,
            "time_base": [1, 30000], "stream_sample_aspect_ratio": [1, 1],
            "codec_parameters_sample_aspect_ratio": [1, 1]}
        spec = CaseSpec(frame_count=1)
        self.assertTrue(inspect_video(spec, observation)["passed"])
        self.assertTrue(inspect_video(spec, {**observation, "codec_parameters_sample_aspect_ratio": [0, 1]})["passed"])
        for key in ("stream_sample_aspect_ratio", "codec_parameters_sample_aspect_ratio"):
            with self.subTest(key=key):
                self.assertFalse(inspect_video(spec, {**observation, key: [2, 1]})["passed"])

    def test_complete_plane_mismatch_is_not_hidden_by_average(self):
        reference = bytes([100]) * 10000
        self.assertTrue(compare_plane(reference, reference)["passed"])
        self.assertFalse(compare_plane(bytes([149]) + reference[1:], reference)["passed"])
        self.assertFalse(compare_plane(bytes([105]) * len(reference), reference)["passed"])
        with self.assertRaises(ValueError):
            compare_plane(reference[:-1], reference)

    def test_pcm_uses_pts_without_event_alignment_and_preserves_priming(self):
        reference = [0.0, 0.0] * 20
        reference[16] = 0.8
        actual = [0.3, -0.3] * 4 + reference + [0.2, -0.2] * 4
        spans = [PcmSpan(0, 8, -4), PcmSpan(8, 20, 4)]
        self.assertTrue(compare_pcm(actual, spans, reference)["passed"])
        shifted = [0.0, 0.0] + actual[:-2]
        self.assertFalse(compare_pcm(shifted, spans, reference)["passed"])
        self.assertFalse(compare_pcm([0.0] * len(actual), spans, reference)["passed"])

    def test_pcm_rejects_holes_overlaps_missing_edges_and_hidden_bytes(self):
        for spans in ([PcmSpan(0, 4, 0), PcmSpan(4, 4, 5)],
                      [PcmSpan(0, 4, 0), PcmSpan(4, 4, 3)],
                      [PcmSpan(0, 8, 1)], [PcmSpan(0, 8, -1)],
                      [PcmSpan(1, 7, 0)]):
            with self.subTest(spans=spans), self.assertRaises(ValueError):
                compare_pcm([0.0] * 16, spans, [0.0] * 16)

    def test_nonfinite_pcm_cannot_pass(self):
        with self.assertRaises(ValueError):
            compare_pcm([float('nan'), 0.0], [PcmSpan(0, 1, 0)], [0.0, 0.0])

    def test_complete_silence_cannot_acquire_quiet_noise(self):
        self.assertFalse(compare_pcm([0.001, 0.001], [PcmSpan(0, 1, 0)], [0.0, 0.0])["passed"])


if __name__ == '__main__':
    unittest.main()
