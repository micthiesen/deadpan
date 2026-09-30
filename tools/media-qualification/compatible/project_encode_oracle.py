"""Bounded emitted project observations against exact clocks and direct inputs."""

from fractions import Fraction
import math

from encoder_oracle import (
    COLOR_METADATA, PcmSpan, _Checks, _effective_frame_sar, _finite_pcm, _integer,
    _ratio, _records,
)


def inspect_video(spec, observation):
    report = _Checks()
    frames = _records(observation["frames"], "project frames", 240)
    clock = _ratio(observation["time_base"], "video clock")
    report.add("exact decoded picture count and complete drain",
               observation["frame_count"] == len(frames) == spec.frame_count
               and observation["decoder_drained"] is True)
    report.add("High profile and zero stream start",
               observation["profile"] == "High" and observation["stream_start_pts"] == 0)
    codec_aspect, codec_aspect_origin = _effective_frame_sar(
        observation["codec_parameters_sample_aspect_ratio"], observation["stream_sample_aspect_ratio"])
    report.add("explicit stream and any present codec SAR agree on square pixels",
               _ratio(observation["stream_sample_aspect_ratio"], "stream SAR") == 1 and codec_aspect == 1,
               {"codec_effective_sar": str(codec_aspect), "codec_sar_origin": codec_aspect_origin})
    report.add("exact stream endpoint",
               _integer(observation["stream_duration"], "video duration", 1) * clock
               == spec.frame_count * spec.frame_duration)
    keys, maximum_b, run = [], 0, 0
    for index, frame in enumerate(frames):
        aspect, _ = _effective_frame_sar(frame["sample_aspect_ratio"], observation["stream_sample_aspect_ratio"])
        report.add(f"picture {index}: exact timing, geometry and interpretation",
                   _integer(frame["pts"], "PTS") * clock == index * spec.frame_duration
                   and frame["best_effort_pts"] == frame["pts"]
                   and _integer(frame["duration"], "duration", 1) * clock == spec.frame_duration
                   and [frame["width"], frame["height"]] == [spec.width, spec.height]
                   and frame["decode_error_flags"] == 0 and not (frame["flags"] & 1)
                   and frame["interlaced"] is False and aspect == 1
                   and all(frame[key] == value for key, value in COLOR_METADATA.items()))
        if frame["keyframe"] is True:
            keys.append(index)
        run = run + 1 if frame["type"] == "B" else 0
        maximum_b = max(maximum_b, run)
    gop = max(1, (spec.fps_num + spec.fps_den) // (2 * spec.fps_den))
    intervals = [right - left for left, right in zip(keys, keys[1:] + [len(frames)])]
    report.add("measured GOP interval and B policy",
               bool(keys) and keys[0] == 0 and max(intervals) <= gop + 1
               and maximum_b <= spec.requested_b_frames
               and (spec.requested_b_frames == 0 or spec.frame_count <= gop or maximum_b > 0),
               {"keyframes": keys, "intervals": intervals, "maximum_b_run": maximum_b})
    return report.result()


def compare_plane(actual, reference):
    """Fixed lossy-stage bounds in 8-bit codes, applied to every complete plane."""
    if not isinstance(actual, bytes) or not isinstance(reference, bytes) or not 0 < len(actual) == len(reference) <= 1920 * 1080:
        raise ValueError("plane extent exceeds bounded equal geometry")
    maximum, total, squares = 0, 0, 0
    for left, right in zip(actual, reference):
        error = abs(left - right)
        maximum = max(maximum, error)
        total += error
        squares += error * error
    mean, mse = total / len(actual), squares / len(actual)
    return {"passed": maximum <= 48 and mean <= 1.5 and mse <= 16,
            "codes": len(actual), "maximum_error": maximum, "mean_absolute_error": mean,
            "mean_squared_error": mse, "psnr_db": None if mse == 0 else 10 * math.log10(255**2 / mse),
            "bounds": {"maximum": 48, "mean_absolute": 1.5, "mean_squared": 16}}


def ffmpeg_spans(observation, pcm):
    count = _finite_pcm(pcm)
    spans = []
    clock = _ratio(observation["time_base"], "audio clock") * 48000
    for frame in _records(observation["frames"], "audio frames", 8192):
        start = _integer(frame["pts"], "audio PTS") * clock
        if start.denominator != 1 or frame["best_effort_pts"] != frame["pts"]:
            raise ValueError("audio frame differs from the exact sample grid")
        if (frame["sample_rate"] != 48000 or frame["channels"] != 2 or frame["channel_layout"] != "stereo"
                or frame["decode_error_flags"] != 0 or frame["flags"] & 1):
            raise ValueError("audio frame interpretation or decode failure")
        spans.append(PcmSpan(frame["sample_offset"], frame["nb_samples"], start.numerator))
    if not spans or spans[-1].sample_offset + spans[-1].sample_count != count:
        raise ValueError("audio frames do not account for all raw PCM")
    return spans


def compare_pcm(actual, spans, reference):
    """Compare on observed absolute timestamps; no event-derived realignment."""
    actual_count, expected_count = _finite_pcm(actual), _finite_pcm(reference)
    if not 0 < expected_count <= 192000 or len(spans) > 8192:
        raise ValueError("PCM fixture exceeds four seconds")
    position = 0
    expected_offset = 0
    previous_end = None
    squares = maximum = 0.0
    signal_squares = 0.0
    for span in spans:
        if (span.sample_offset != expected_offset or span.sample_offset + span.sample_count > actual_count
                or previous_end is not None and span.start_sample != previous_end):
            raise ValueError("PCM spans have a gap, overlap, or invalid physical offset")
        expected_offset += span.sample_count
        previous_end = span.start_sample + span.sample_count
        low, high = max(0, span.start_sample), min(expected_count, previous_end)
        if high <= low:
            continue
        if low != position:
            raise ValueError("decoded PCM does not cover the authored interval")
        for sample in range(low, high):
            offset = span.sample_offset + sample - span.start_sample
            for channel in range(2):
                desired = reference[sample * 2 + channel]
                error = actual[offset * 2 + channel] - desired
                maximum = max(maximum, abs(error))
                squares += error * error
                signal_squares += desired * desired
        position = high
    if position != expected_count or expected_offset != actual_count:
        raise ValueError("decoded PCM omits authored content or physical data")
    rms = math.sqrt(squares / (2 * expected_count))
    reference_rms = math.sqrt(signal_squares / (2 * expected_count))
    return {"passed": maximum <= 0.25 and rms <= 0.02 and (signal_squares != 0 or maximum == 0),
            "authored_samples": expected_count, "physical_samples": actual_count,
            "maximum_error": maximum, "rms_error": rms, "reference_rms": reference_rms,
            "complete_silence": signal_squares == 0,
            "bounds": {"maximum_error": 0.25, "rms_error": 0.02},
            "mapping": "unmodified PCM at observed absolute sample PTS; no alignment or gain adjustment"}
