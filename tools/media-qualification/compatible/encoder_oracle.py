"""Bounded, independent observations for the developer CFR encoder experiment.

No codec, filesystem, subprocess or GPU access occurs here. The host supplies
bounded probe dictionaries and unmodified interleaved stereo PCM. A scoped pass
does not qualify edit-list policy, closed GOPs, edge-content survival, a second
decoder implementation, or the renderer-to-encoder color transform.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from fractions import Fraction
import math


SAMPLE_RATE = 48000
MAX_FRAMES = 240
MAX_AUDIO_SAMPLES = 120 * SAMPLE_RATE
MAX_DECODED_SAMPLES = MAX_AUDIO_SAMPLES + 8192
MAX_AUDIO_FRAMES = 16384
MAX_PACKETS = 32768
SEARCH_RADIUS = 4096
NEUTRAL_LEVELS = ("0.001", "0.01", "0.018", "0.18", "0.5")
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
U32_MAX = (1 << 32) - 1
# Pinned FFmpeg libavutil/frame.h: CORRUPT is 1 << 0; KEY is 1 << 1.
AV_FRAME_FLAG_CORRUPT = 1 << 0
COLOR_METADATA = {
    "color_range": "tv", "color_space": "bt709", "color_transfer": "bt709",
    "color_primaries": "bt709", "chroma_location": "left",
}


class OracleError(ValueError):
    """Invalid or oversized input to this deliberately bounded oracle."""


def _integer(value, label, minimum=I64_MIN, maximum=I64_MAX, *, text=False):
    if text and isinstance(value, str) and len(value) <= 21 and value.isascii():
        digits = value[1:] if value.startswith("-") else value
        if digits and digits.isdigit():
            value = int(value)
    if type(value) is not int or not minimum <= value <= maximum:
        raise OracleError(f"{label} must be an integer in {minimum}..{maximum}")
    return value


def _ratio(value, label):
    if isinstance(value, str) and len(value) <= 43:
        separator = ":" if ":" in value else "/"
        value = value.split(separator)
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise OracleError(f"{label} must be a positive rational pair")
    return Fraction(_integer(value[0], label, 1, U32_MAX, text=True),
                    _integer(value[1], label, 1, U32_MAX, text=True))


def _mapping(value, label):
    if not isinstance(value, Mapping):
        raise OracleError(f"{label} must be an object")
    return value


def _records(value, label, maximum):
    if not isinstance(value, (list, tuple)) or len(value) > maximum:
        raise OracleError(f"{label} must be an array of at most {maximum} records")
    return value


def _exact(value: Fraction):
    return value.numerator if value.denominator == 1 else str(value)


def _round_even(value: Fraction) -> int:
    floor, remainder = divmod(value.numerator, value.denominator)
    doubled = remainder * 2
    return floor + int(doubled > value.denominator or
                       (doubled == value.denominator and floor % 2 != 0))


def _rate(numerator, denominator):
    return Fraction(_integer(numerator, "fps numerator", 1, U32_MAX),
                    _integer(denominator, "fps denominator", 1, U32_MAX))


def audio_boundary(frame: int, fps_num: int, fps_den: int) -> int:
    """Match core FrameRate::audio_boundary, including signed ties-to-even."""
    frame = _integer(frame, "project frame")
    result = _round_even(Fraction(frame * SAMPLE_RATE, 1) / _rate(fps_num, fps_den))
    return _integer(result, "audio boundary")


def point_ceil_boundary(frame: int, fps_num: int, fps_den: int) -> int:
    """Definition-output point grid, deliberately distinct from root allocation."""
    frame = _integer(frame, "project frame")
    value = Fraction(frame * SAMPLE_RATE, 1) / _rate(fps_num, fps_den)
    return _integer(-(-value.numerator // value.denominator), "point-ceil boundary")


def movie_timescale(fps_num: int, fps_den: int) -> int:
    scale = math.lcm(_rate(fps_num, fps_den).numerator, SAMPLE_RATE)
    return _integer(scale, "movie timescale", 1, (1 << 31) - 1)


@dataclass(frozen=True)
class CaseSpec:
    frame_count: int = 120
    fps_num: int = 30000
    fps_den: int = 1001
    pcm_kind: str = "impulses"
    requested_b_frames: int = 0
    width: int = 320
    height: int = 180

    def __post_init__(self):
        _integer(self.frame_count, "frame count", 1, MAX_FRAMES)
        rate = _rate(self.fps_num, self.fps_den)
        if not 1 <= rate <= 60:
            raise OracleError("experiment frame rate must be in 1..60")
        object.__setattr__(self, "fps_num", rate.numerator)
        object.__setattr__(self, "fps_den", rate.denominator)
        _integer(self.width, "width", 2, 8192)
        _integer(self.height, "height", 2, 8192)
        if self.width % 2 or self.height % 2:
            raise OracleError("4:2:0 fixture dimensions must be even")
        if self.pcm_kind not in ("impulses", "edges"):
            raise OracleError("PCM fixture must be impulses or edges")
        if type(self.requested_b_frames) is not int or self.requested_b_frames not in (0, 2):
            raise OracleError("requested B-frame bound must be 0 or 2")
        _integer(self.audio_samples, "authored audio samples", 1, MAX_AUDIO_SAMPLES)
        movie_timescale(self.fps_num, self.fps_den)

    @property
    def frame_duration(self):
        return Fraction(self.fps_den, self.fps_num)

    @property
    def frame_samples(self):
        return self.frame_duration * SAMPLE_RATE

    @property
    def audio_samples(self):
        return audio_boundary(self.frame_count, self.fps_num, self.fps_den)


def expected_manifest(spec: CaseSpec) -> dict:
    """Independent authored input contract; never derived from encoder output."""
    if not isinstance(spec, CaseSpec):
        raise OracleError("spec must be a CaseSpec")
    count = spec.audio_samples
    return {
        "frame_count": spec.frame_count, "frame_rate": [spec.fps_num, spec.fps_den],
        "time_base": [1, spec.fps_num], "start_pts": 0,
        "duration_ticks": spec.frame_count * spec.fps_den,
        "video_duration": str(spec.frame_count * spec.frame_duration),
        "audio_samples": count, "audio_offset_samples": 0,
        "definition_point_ceil_samples": point_ceil_boundary(spec.frame_count, spec.fps_num, spec.fps_den),
        "movie_timescale": movie_timescale(spec.fps_num, spec.fps_den),
        "width": spec.width, "height": spec.height, "pcm_kind": spec.pcm_kind,
        "requested_b_frames": spec.requested_b_frames,
        "impulses": [100, min(SAMPLE_RATE, count // 2), count - 200],
    }


def bt709_oetf(linear: float) -> float:
    if type(linear) not in (int, float) or not 0 <= linear <= 1 or not math.isfinite(linear):
        raise OracleError("linear-light level must be finite and in 0..1")
    return 4.5 * linear if linear < 0.018 else 1.099 * linear ** 0.45 - 0.099


def color_reference() -> dict:
    """Independent f64 BT.709 matrix and transfer oracle, before lossy coding."""
    colors = []
    for red, green, blue in ((0, 0, 0), (1, 1, 1), (1, 0, 0), (0, 1, 0), (0, 0, 1)):
        y = 0.2126 * red + 0.7152 * green + 0.0722 * blue
        colors.append([math.floor(16 + 219 * y + 0.5),
                       math.floor(128 + 112 * (blue - y) / (1 - 0.0722) + 0.5),
                       math.floor(128 + 112 * (red - y) / (1 - 0.2126) + 0.5)])
    neutrals = [[math.floor(16 + 219 * bt709_oetf(float(Fraction(level))) + 0.5), 128, 128]
                for level in NEUTRAL_LEVELS]
    return {"color_yuv_patches": colors, "neutral_yuv_patches": neutrals,
            "neutral_linear_levels": list(NEUTRAL_LEVELS)}


class _Checks:
    def __init__(self):
        self.checks = []
        self.observations = {}
        self.unqualified = []

    def add(self, label, passed, details=None, *, diagnostic=False):
        self.checks.append({"label": label, "passed": bool(passed), "details": details,
                            "diagnostic": diagnostic})

    def section(self, label, function):
        try:
            function()
        except (OracleError, KeyError) as error:
            self.add(f"{label}: valid bounded observations", False, str(error))

    def result(self):
        return {"passed": all(check["passed"] for check in self.checks if not check["diagnostic"]),
                "checks": self.checks, "observations": self.observations,
                "scope": "CFR input, decoded timing/identity/color and absolute audio-event observations",
                "unqualified": ["no-edit-list/fast-start policy (separate structural inspector)",
                                "closed GOP independence", "edge-content survival against same-payload reference",
                                "second decoder implementation", "renderer-to-encoder color conversion"] + self.unqualified}


def _input_checks(report, spec, source):
    source = _mapping(source, "encoder manifest")
    expected = expected_manifest(spec)
    report.observations["expected_manifest"] = expected
    for key in ("frame_count", "start_pts", "duration_ticks", "audio_samples", "audio_offset_samples",
                "movie_timescale", "width", "height", "requested_b_frames"):
        report.add(f"input: exact {key}", _integer(source[key], key) == expected[key],
                   {"expected": expected[key], "actual": source[key]})
    for key in ("time_base", "frame_rate"):
        report.add(f"input: exact {key}", _ratio(source[key], key) == Fraction(*expected[key]),
                   {"expected": expected[key], "actual": source[key]})
    report.add("input: PCM fixture", source["pcm_kind"] == spec.pcm_kind)
    impulses = [_integer(value, "impulse", 0, MAX_AUDIO_SAMPLES)
                for value in _records(source["impulses"], "impulses", 3)]
    report.add("input: exact authored impulses", impulses == expected["impulses"],
               {"expected": expected["impulses"], "actual": impulses})


def _patches(value, label):
    patches = _records(value, label, 5)
    if len(patches) != 5:
        raise OracleError(f"{label} must contain all five patches")
    result = []
    for patch in patches:
        if not isinstance(patch, (list, tuple)) or len(patch) != 3:
            raise OracleError(f"{label} requires three YUV codes per patch")
        result.append([_integer(code, label, 0, 255) for code in patch])
    return result


def _color_checks(report, source, video):
    expected = color_reference()
    report.observations["color_reference"] = expected
    report.add("color: declared independent linear levels",
               source["neutral_linear_levels"] == list(NEUTRAL_LEVELS))
    for key, decoded in (("color_yuv_patches", "first_frame_yuv_patches"),
                         ("neutral_yuv_patches", "first_frame_neutral_patches")):
        before = _patches(source[key], key)
        after = _patches(video[decoded], decoded)
        report.add(f"color: exact pre-encode {key}", before == expected[key],
                   {"expected": expected[key], "actual": before})
        error = max(abs(actual - reference) for patch, target in zip(after, expected[key])
                    for actual, reference in zip(patch, target))
        report.add(f"color: decoded {key} within four codes", error <= 4,
                   {"maximum_error": error, "actual": after, "expected": expected[key]})


def _metadata_checks(report, spec, probe):
    streams = [_mapping(stream, "stream") for stream in _records(probe["streams"], "streams", 16)]
    video = [stream for stream in streams if stream.get("codec_type") == "video"]
    audio = [stream for stream in streams if stream.get("codec_type") == "audio"]
    report.add("metadata: exactly one video and audio stream", len(video) == len(audio) == 1)
    if len(video) != 1 or len(audio) != 1:
        return
    video, audio = video[0], audio[0]
    required = {"codec_name": "h264", "profile": "High", "pix_fmt": "yuv420p",
                "field_order": "progressive", **COLOR_METADATA}
    for key, expected in required.items():
        report.add(f"metadata: video {key}", video.get(key) == expected,
                   {"expected": expected, "actual": video.get(key)})
    for key in ("width", "height"):
        report.add(f"metadata: video {key}", _integer(video.get(key), key, 1, 8192, text=True) == getattr(spec, key))
    report.add("metadata: square pixels", _ratio(video.get("sample_aspect_ratio"), "video SAR") == 1)
    for key, expected in {"codec_name": "aac", "profile": "LC", "channel_layout": "stereo"}.items():
        report.add(f"metadata: audio {key}", audio.get(key) == expected,
                   {"expected": expected, "actual": audio.get(key)})
    report.add("metadata: audio 48000 Hz", _integer(audio.get("sample_rate"), "audio sample rate", 1, 384000, text=True) == SAMPLE_RATE)
    report.add("metadata: audio stereo", _integer(audio.get("channels"), "channels", 1, 64, text=True) == 2)


def _effective_frame_sar(raw, stream):
    if not isinstance(raw, (list, tuple)) or len(raw) != 2:
        raise OracleError("frame SAR must contain a numerator and denominator")
    numerator = _integer(raw[0], "frame SAR numerator", 0, U32_MAX)
    denominator = _integer(raw[1], "frame SAR denominator", 1, U32_MAX)
    if numerator == 0:
        return _ratio(stream, "explicit stream SAR"), "stream"
    return Fraction(numerator, denominator), "frame"


def _video_checks(report, spec, video):
    video = _mapping(video, "video")
    frames = [_mapping(frame, "video frame") for frame in _records(video["frames"], "video frames", MAX_FRAMES)]
    tb = _ratio(video["time_base"], "video time base")
    report.add("video: exact frame count", len(frames) == spec.frame_count,
               {"expected": spec.frame_count, "actual": len(frames)})
    if not frames:
        return
    identities, pts, durations, keyframes, b_runs = [], [], [], [], []
    b_run = 0
    color_failures, field_failures, errors, aspects = [], [], [], []
    for number, frame in enumerate(frames):
        identities.append(_integer(frame["authored_identity"], "visible frame identity", 0, MAX_FRAMES))
        pts.append(_integer(frame["pts"], "video PTS") * tb)
        durations.append(_integer(frame["duration"], "video duration", 0) * tb)
        best = _integer(frame["best_effort_pts"], "video best-effort PTS") * tb
        if best != pts[-1]:
            errors.append({"frame": number, "pts": str(pts[-1]), "best_effort_pts": str(best)})
        if _integer(frame["decode_error_flags"], "video decode error flags", 0) != 0:
            errors.append({"frame": number, "decode_error_flags": frame["decode_error_flags"]})
        if _integer(frame["flags"], "video frame flags", 0, U32_MAX) & AV_FRAME_FLAG_CORRUPT:
            errors.append({"frame": number, "flags": frame["flags"], "error": "corrupt frame flag"})
        if frame.get("keyframe") is True:
            keyframes.append(number)
        b_run = b_run + 1 if frame.get("type") == "B" else 0
        b_runs.append(b_run)
        aspect = {"frame": number, "raw": frame.get("sample_aspect_ratio"), "effective": None}
        try:
            effective, origin = _effective_frame_sar(aspect["raw"], video.get("stream_sample_aspect_ratio"))
            aspect.update({"effective": str(effective), "source": origin})
        except OracleError as error:
            effective = None
            aspect["error"] = str(error)
        aspects.append(aspect)
        if frame.get("interlaced") is not False or effective != 1:
            field_failures.append(number)
        for key, expected in COLOR_METADATA.items():
            if frame.get(key) != expected:
                color_failures.append({"frame": number, "field": key, "actual": frame.get(key)})
    expected_pts = [number * spec.frame_duration for number in range(spec.frame_count)]
    report.add("video: every visible authored identity", identities == list(range(spec.frame_count)),
               {"actual": identities})
    report.add("video: exact rational CFR PTS", pts == expected_pts,
               {"actual": [str(value) for value in pts], "expected": [str(value) for value in expected_pts]})
    report.add("video: strictly increasing presentation PTS", all(left < right for left, right in zip(pts, pts[1:])))
    report.add("video: exact per-frame durations", durations == [spec.frame_duration] * spec.frame_count,
               {"actual": [str(value) for value in durations]})
    terminal = pts[-1] + durations[-1]
    report.add("video: exact terminal boundary", terminal == spec.frame_count * spec.frame_duration,
               {"actual": str(terminal), "expected": str(spec.frame_count * spec.frame_duration)})
    report.add("video: no concealed decode/timestamp errors", not errors, errors)
    report.add("video: progressive square-pixel frames", not field_failures, field_failures)
    report.add("video: decoded color interpretation", not color_failures, color_failures)
    report.add("video: starts with a key picture", bool(keyframes) and keyframes[0] == 0)
    maximum_b = max(b_runs, default=0)
    report.add("video: actual B run within requested maximum", maximum_b <= spec.requested_b_frames,
               {"requested": spec.requested_b_frames, "actual": maximum_b})
    intervals = [right - left for left, right in zip(keyframes, keyframes[1:] + [len(frames)])]
    report.observations["video"] = {"keyframes": keyframes, "keyframe_intervals": intervals,
        "maximum_b_run": maximum_b, "closed_gop_qualified": False,
        "stream_sample_aspect_ratio": video.get("stream_sample_aspect_ratio"),
        "codec_parameters_sample_aspect_ratio": video.get("codec_parameters_sample_aspect_ratio"),
        "frame_sample_aspect_ratios": aspects,
        "terminal_seconds": str(terminal), "raw_frame_durations": [str(value) for value in durations]}


def _packet_checks(report, spec, packets):
    packets = _mapping(packets, "packet report")
    records = [_mapping(packet, "packet") for packet in _records(packets["packets"], "packets", MAX_PACKETS)]
    video = [packet for packet in records if packet.get("codec_type") == "video"]
    dts, pts = [], []
    for packet in video:
        tb = _ratio(packet["time_base"], "packet time base")
        dts.append(_integer(packet["dts"], "video packet DTS") * tb)
        pts.append(_integer(packet["pts"], "video packet PTS") * tb)
    report.add("packets: complete CFR video packet inventory", len(video) == spec.frame_count)
    report.add("packets: strictly increasing video DTS", bool(dts) and all(left < right for left, right in zip(dts, dts[1:])))
    report.add("packets: video PTS retain authored presentation set", sorted(pts) == [n * spec.frame_duration for n in range(spec.frame_count)])
    report.observations["packets"] = {"video_dts": [str(value) for value in dts],
        "video_pts": [str(value) for value in pts], "presentation_reordering_observed": pts != sorted(pts),
        "idr_is_not_closed_gop_proof": True}


def _finite_pcm(pcm):
    if not isinstance(pcm, Sequence) or isinstance(pcm, (str, bytes, bytearray)):
        raise OracleError("PCM must be an interleaved numeric stereo sequence")
    if len(pcm) % 2 or len(pcm) > MAX_DECODED_SAMPLES * 2:
        raise OracleError("PCM exceeds the stereo sample bound or has an odd value count")
    for value in pcm:
        if (type(value) not in (int, float)
                or (type(value) is int and not I64_MIN <= value <= I64_MAX)
                or not math.isfinite(value)):
            raise OracleError("PCM contains nonfinite or nonnumeric values")
    return len(pcm) // 2


@dataclass(frozen=True)
class PcmSpan:
    """A contiguous raw PCM region on an absolute 48000 Hz sample clock."""

    sample_offset: int
    sample_count: int
    start_sample: int

    def __post_init__(self):
        _integer(self.sample_offset, "PCM span offset", 0, MAX_DECODED_SAMPLES)
        _integer(self.sample_count, "PCM span count", 1, MAX_DECODED_SAMPLES)
        _integer(self.start_sample, "PCM span start")
        _integer(self.sample_offset + self.sample_count, "PCM span buffer end", 1, MAX_DECODED_SAMPLES)
        _integer(self.start_sample + self.sample_count, "PCM span clock end")


def inspect_pcm_events(spec: CaseSpec, pcm: Sequence[float], spans: Sequence[PcmSpan],
                       *, tolerance_samples: int = 0, label: str = "pcm") -> dict:
    """Measure absolute authored events without decoder/container assumptions.

    Input is unmodified interleaved stereo PCM and complete ordered spans.
    Invalid, discontinuous or overlapping layouts raise before peak scanning.
    An invalid tolerance still retains signed event diagnostics, with failed
    acceptance. No timestamp alignment, trimming or normalization occurs.
    """
    if not isinstance(spec, CaseSpec):
        raise OracleError("spec must be a CaseSpec")
    if not isinstance(label, str) or not label or len(label) > 64:
        raise OracleError("PCM event label must contain 1..64 characters")
    spans = _records(spans, "PCM spans", MAX_AUDIO_FRAMES)
    count = _finite_pcm(pcm)
    if not spans or count == 0:
        raise OracleError("PCM event measurement requires nonempty PCM and spans")
    next_offset, next_sample = 0, None
    segments = []
    for span in spans:
        if not isinstance(span, PcmSpan):
            raise OracleError("PCM spans must be PcmSpan values")
        if span.sample_offset != next_offset or (next_sample is not None and span.start_sample != next_sample):
            raise OracleError("PCM spans must cover contiguous bytes and absolute sample clocks")
        next_offset = span.sample_offset + span.sample_count
        next_sample = span.start_sample + span.sample_count
        if next_offset > count:
            raise OracleError("PCM span exceeds supplied PCM")
        segments.append((span.sample_offset, span.sample_count, Fraction(span.start_sample)))
    if next_offset != count:
        raise OracleError("PCM spans must account for every supplied sample")
    report = _Checks()
    tolerance_valid = type(tolerance_samples) is int and 0 <= tolerance_samples < spec.frame_samples
    tolerance = tolerance_samples if type(tolerance_samples) is int else 0
    first = segments[0][2]
    physical_end = segments[-1][2] + segments[-1][1]
    events = []
    expected = expected_manifest(spec)["impulses"]
    independent_events = all(right - left > SEARCH_RADIUS * 2
                             for left, right in zip(expected, expected[1:]))
    if not independent_events:
        report.unqualified.append(f"{label} event timing: authored marker diagnostic windows overlap")
    report.add(f"{label}: independent event diagnostic windows", independent_events,
               {"expected_samples": expected, "search_radius_samples": SEARCH_RADIUS}, diagnostic=True)
    for channel, name in enumerate(("left", "right")):
        selected = []
        for expected_sample in expected:
            peak = None
            searched = 0
            for offset, samples, start in segments:
                low = max(0, math.ceil(expected_sample - SEARCH_RADIUS - start))
                high = min(samples, math.floor(expected_sample + SEARCH_RADIUS - start) + 1)
                for local in range(low, high):
                    amplitude = pcm[(offset + local) * 2 + channel]
                    searched += 1
                    if peak is None or abs(amplitude) > abs(peak[1]):
                        peak = (start + local, amplitude)
            actual = peak[0] if peak is not None else None
            amplitude = peak[1] if peak is not None else None
            error = actual - expected_sample if actual is not None else None
            signal = amplitude is not None and abs(amplitude) > 0.15 and (amplitude > 0 if channel == 0 else amplitude < 0)
            exact = signal and error == 0
            within = signal and tolerance_valid and abs(error) <= tolerance and abs(error) < spec.frame_samples
            event = {"channel": name, "expected_sample": expected_sample,
                     "actual_peak_sample": _exact(actual) if actual is not None else None,
                     "error_samples": _exact(error) if error is not None else None,
                     "error_seconds": str(error / SAMPLE_RATE) if error is not None else None,
                     "peak_amplitude": amplitude, "searched_samples": searched,
                     "search_radius_samples": SEARCH_RADIUS, "exact": exact, "within_tolerance": within}
            events.append(event)
            selected.append(actual)
            report.add(f"{label}: {name} event {expected_sample} exact", exact, event, diagnostic=True)
            report.add(f"{label}: {name} event {expected_sample} within declared sub-frame tolerance",
                       within, event, diagnostic=not independent_events)
        report.add(f"{label}: {name} events select distinct measured peaks", len(set(selected)) == len(expected) and None not in selected,
                   {"selected_samples": [_exact(value) if value is not None else None for value in selected]},
                   diagnostic=not independent_events)
    observations = {"events": events, "physical_start_sample": _exact(first),
        "physical_end_sample": _exact(physical_end), "physical_tail_after_authored_end": _exact(physical_end - spec.audio_samples),
        "exact_event_samples": all(event["exact"] for event in events),
        "event_timing_qualified": independent_events and all(event["within_tolerance"] for event in events),
        "edge_content_qualified": False, "alignment_or_event_based_cropping_applied": False}
    return {"passed": tolerance_valid and all(check["passed"] for check in report.checks if not check["diagnostic"]),
            "checks": report.checks, "observations": observations, "unqualified": report.unqualified}


def _audio_checks(report, label, spec, audio, pcm, tolerance, tolerance_valid):
    audio = _mapping(audio, label)
    count = _finite_pcm(pcm)
    tb = _ratio(audio["time_base"], "audio time base")
    frames = [_mapping(frame, "audio frame") for frame in _records(audio["frames"], "audio frames", MAX_AUDIO_FRAMES)]
    report.add(f"{label}: explicit decoder mode", audio.get("mode") == label)
    report.add(f"{label}: 48000 Hz stereo", audio.get("sample_rate") == SAMPLE_RATE
               and audio.get("channels") == 2 and audio.get("channel_layout") == "stereo")
    report.add(f"{label}: declared PCM count", _integer(audio["decoded_samples"], "decoded samples", 0, MAX_DECODED_SAMPLES) == count)
    report.add(f"{label}: declared decoded frame count", _integer(audio["decoded_frames"], "decoded frames", 0, MAX_AUDIO_FRAMES) == len(frames))
    report.add(f"{label}: nonempty decoded PCM", bool(frames) and count > 0)
    segments, bad, skips, offsets = [], [], [], []
    next_offset, next_sample = 0, None
    for number, frame in enumerate(frames):
        offset = _integer(frame["sample_offset"], "PCM frame offset", 0, MAX_DECODED_SAMPLES)
        samples = _integer(frame["nb_samples"], "audio frame samples", 1, 8192)
        if offset + samples > count:
            raise OracleError("audio frame exceeds supplied PCM")
        start = _integer(frame["pts"], "audio frame PTS") * tb * SAMPLE_RATE
        best = _integer(frame["best_effort_pts"], "audio best-effort PTS") * tb * SAMPLE_RATE
        if offset != next_offset or (next_sample is not None and start != next_sample):
            offsets.append(number)
        if start.denominator != 1 or best != start:
            bad.append({"frame": number, "pts_samples": str(start), "best_effort_samples": str(best)})
        if frame.get("sample_rate") != SAMPLE_RATE or frame.get("channels") != 2 or frame.get("channel_layout") != "stereo":
            bad.append({"frame": number, "error": "audio frame interpretation changed"})
        if _integer(frame["decode_error_flags"], "audio decode error flags", 0) != 0:
            bad.append({"frame": number, "decode_error_flags": frame["decode_error_flags"]})
        if _integer(frame["flags"], "audio frame flags", 0, U32_MAX) & AV_FRAME_FLAG_CORRUPT:
            bad.append({"frame": number, "flags": frame["flags"], "error": "corrupt frame flag"})
        skip = frame.get("skip")
        leading, trailing = 0, 0
        if skip is not None:
            skip = _mapping(skip, "audio skip metadata")
            leading = _integer(skip["leading"], "leading skip", 0, U32_MAX)
            trailing = _integer(skip["trailing"], "trailing skip", 0, U32_MAX)
            skips.append({"frame": number, "leading": leading, "trailing": trailing,
                          "leading_reason": skip.get("leading_reason"), "trailing_reason": skip.get("trailing_reason")})
        segments.append((offset, samples, start))
        next_offset, next_sample = offset + samples, start + samples
    contiguous = not offsets and next_offset == count
    report.add(f"{label}: contiguous absolute sample frames", contiguous,
               {"bad_frames": offsets, "accounted_samples": next_offset, "pcm_samples": count})
    report.add(f"{label}: exact sample grid and no decode errors", not bad, bad)
    if not segments:
        return
    first = segments[0][2]
    report.add(f"{label}: first-sample summary agrees with raw PTS", _integer(audio["first_sample_pts"], "first sample PTS") == first)
    stream_start = _integer(audio["stream_start_pts"], "audio stream start") * tb * SAMPLE_RATE
    stream_duration = _integer(audio["stream_duration"], "audio stream duration", 0) * tb * SAMPLE_RATE
    stream_end = stream_start + stream_duration
    end_error = stream_end - spec.audio_samples
    report.add(f"{label}: exact authored stream endpoint", end_error == 0,
               {"error_samples": _exact(end_error)}, diagnostic=True)
    report.add(f"{label}: stream endpoint within declared sub-frame tolerance",
               tolerance_valid and abs(end_error) <= tolerance and abs(end_error) < spec.frame_samples,
               {"error_samples": _exact(end_error), "declared_tolerance_samples": tolerance})
    # Invalid overlap/gap reports must not multiply scanning work by referencing
    # the same PCM repeatedly, or claim events on an invented sample timeline.
    if not contiguous or bad:
        report.observations[label] = {"event_timing_qualified": False,
            "event_search_skipped": "invalid absolute PCM frame layout",
            "stream_end_error_samples": _exact(end_error), "frame_skip_metadata": skips}
        return
    measured = inspect_pcm_events(spec, pcm,
        [PcmSpan(offset, samples, start.numerator) for offset, samples, start in segments],
        tolerance_samples=tolerance if tolerance_valid else -1, label=label)
    report.checks.extend(measured["checks"])
    report.unqualified.extend(measured["unqualified"])
    report.observations[label] = {**measured["observations"],
        "stream_start_sample": _exact(stream_start), "stream_end_sample": _exact(stream_end),
        "stream_end_error_samples": _exact(end_error), "frame_skip_metadata": skips,
        "discarded_frames": [number for number, frame in enumerate(frames) if frame.get("discard") is True]}


def inspect_case(
    spec: CaseSpec, source: dict, video: dict, ordinary_audio: dict, manual_audio: dict,
    probe: dict, *, ordinary_pcm: Sequence[float], manual_pcm: Sequence[float],
    tolerance_samples: int = 0, packets: dict | None = None,
) -> dict:
    """Collect independent checks; malformed sections fail without hiding others.

    Exact-sample checks are diagnostics alongside acceptance under the explicitly
    declared encoded tolerance. That tolerance must remain strictly below one
    output frame. Absolute timestamps and complete emitted PCM are never aligned,
    shifted, normalized or cropped by an observed event.
    """
    if not isinstance(spec, CaseSpec):
        raise OracleError("spec must be a CaseSpec")
    report = _Checks()
    valid = type(tolerance_samples) is int and 0 <= tolerance_samples < spec.frame_samples
    report.add("timing: declared encoded tolerance strictly below one frame", valid,
               {"tolerance_samples": tolerance_samples, "one_frame_samples": str(spec.frame_samples)})
    # Preserve event diagnostics even when the supplied tolerance is invalid.
    tolerance = tolerance_samples if type(tolerance_samples) is int else 0
    report.section("input", lambda: _input_checks(report, spec, source))
    report.section("color", lambda: _color_checks(report, _mapping(source, "source"), _mapping(video, "video")))
    report.section("metadata", lambda: _metadata_checks(report, spec, _mapping(probe, "ffprobe")))
    report.section("video", lambda: _video_checks(report, spec, video))
    if packets is not None:
        report.section("packets", lambda: _packet_checks(report, spec, packets))
    for label, audio, pcm in (("ordinary", ordinary_audio, ordinary_pcm), ("manual", manual_audio, manual_pcm)):
        report.section(label, lambda label=label, audio=audio, pcm=pcm: _audio_checks(report, label, spec, audio, pcm, tolerance, valid))
    return report.result()
