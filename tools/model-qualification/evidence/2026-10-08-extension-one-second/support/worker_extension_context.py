"""Strict retained extension evidence, independent of inference and bridge input.

The host recaptures source facts and recomputes shot/region policy. This reader
checks the wire binding, complete signature coverage and immutable binary shape;
it cannot establish that observations came from the claimed media.
"""

from fractions import Fraction
import hashlib
import json
from math import gcd
import re
import struct

from worker_media import exact_keys, integer
from worker_protocol import WorkspaceArtifact, _core_identifier

MAX_SIGNATURES = 512
MAX_SIGNATURE_BYTES = 950284
SIGNATURE_BYTES = 1856
CONTEXT_PADDING = 125  # deadpan-context-shots-1: 5 * maximum half-span (25).
U32_MAX = (1 << 32) - 1
U64_MAX = (1 << 64) - 1
RUST_WHITESPACE = "\t\n\v\f\r \u0085\u00a0\u1680" + "".join(chr(value) for value in range(0x2000, 0x200B)) + "\u2028\u2029\u202f\u205f\u3000"


def exact_ratio(value):
    exact_keys(value, ["numerator", "denominator"])
    numbers = []
    for key in ("numerator", "denominator"):
        text = value[key]
        if (not isinstance(text, str) or len(text) > 40
                or re.fullmatch(r"[+-]?[0-9]+", text) is None):
            raise ValueError("invalid exact input coordinate")
        numbers.append(integer(int(text), -(1 << 127), (1 << 127) - 1))
    if numbers[1] <= 0:
        raise ValueError("input coordinate denominator must be positive")
    return Fraction(*numbers)


def positive_rate(value):
    exact_keys(value, ["numerator", "denominator"])
    return Fraction(integer(value["numerator"], 1, U32_MAX),
                    integer(value["denominator"], 1, U32_MAX))


def digest(value):
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise ValueError("invalid retained input digest")
    return value


def object_ref(value):
    exact_keys(value, ["content", "byte_length"])
    exact_keys(value["content"], ["algorithm", "digest"])
    if value["content"]["algorithm"] != "blake3":
        raise ValueError("generated input content requires blake3")
    return (digest(value["content"]["digest"]), integer(value["byte_length"], 1, U64_MAX))


def picture_identity(value):
    """Return a hashable provider plus ordinal, after validating its wire shape."""
    if not isinstance(value, dict):
        raise ValueError("invalid generation picture identity")
    kind = value.get("kind")
    if kind == "authored_black":
        exact_keys(value, ["kind"])
        return (kind,), None
    if kind == "original":
        exact_keys(value, ["kind", "qualification", "frame"])
        provider = (kind, digest(value["qualification"]))
    elif kind == "generated":
        exact_keys(value, ["kind", "sampled_object", "frame", "content_aspect"])
        aspect = value["content_aspect"]
        if aspect is not None:
            if not isinstance(aspect, list) or len(aspect) != 2:
                raise ValueError("invalid generated content aspect")
            aspect = tuple(integer(side, 1, U32_MAX) for side in aspect)
            if gcd(*aspect) != 1:
                raise ValueError("generated content aspect must be reduced")
        provider = (kind, object_ref(value["sampled_object"]), aspect)
    else:
        raise ValueError("unsupported generation picture identity")
    return provider, integer(value["frame"], 0, U64_MAX)


def timestamp(value):
    exact_keys(value, ["ticks", "time_base"])
    ticks = integer(value["ticks"], -(1 << 63), (1 << 63) - 1)
    base = positive_rate(value["time_base"])
    return {"ticks": ticks, "time_base": {"numerator": base.numerator, "denominator": base.denominator}}


def boundary_identity(boundary):
    # The outer boundary/clock/geometry grammar is checked by worker.py.
    kind, value = next(iter(boundary.items()))
    if kind == "authored_black":
        return (kind,), None
    if kind == "original":
        _core_identifier(value["asset"], "Original asset")
        provider = (kind, digest(value["qualification"]))
    else:
        _core_identifier(value["sampled_asset"], "sampled asset")
        object_ref(value["provenance"])
        provider = (kind, object_ref(value["sampled_object"]))
    decoded = value["picture"]
    exact_keys(decoded, ["source_frame", "pts", "stream", "model_input"])
    timestamp(decoded["pts"])
    stream = decoded["stream"]
    exact_keys(stream, ["codec", "pixel_format", "width", "height", "sample_aspect",
                        "rotation_quarter_turns", "decoded_sample_bits", "color"])
    for key in ("codec", "pixel_format"):
        text = stream[key]
        if not isinstance(text, str) or not 1 <= len(text) <= 64 or any(not 33 <= ord(c) <= 126 for c in text):
            raise ValueError("invalid measured stream label")
    for key in ("width", "height"):
        integer(stream[key], 1, 8192)
    aspect = stream["sample_aspect"]
    if not isinstance(aspect, list) or len(aspect) != 2:
        raise ValueError("invalid measured stream aspect")
    for side in aspect:
        integer(side, 1, U32_MAX)
    integer(stream["rotation_quarter_turns"], 0, 3)
    if integer(stream["decoded_sample_bits"], 8, 16) not in (8, 16):
        raise ValueError("invalid decoded sample depth")
    color = stream["color"]
    vocab = {"transfer": ("bt709", "srgb", "linear", "pq", "hlg"),
             "primaries": ("bt709", "bt2020", "display_p3"),
             "matrix": ("rgb", "bt709", "bt601", "bt2020_ncl"), "range": ("limited", "full")}
    exact_keys(color, vocab)
    if any(color[key] not in choices for key, choices in vocab.items()):
        raise ValueError("invalid measured stream color")
    conversion = {"srgb": "srgb_codes_unchanged", "bt709": "rec709_to_srgb"}.get(color["transfer"])
    if (stream["rotation_quarter_turns"] != 0 or stream["decoded_sample_bits"] != 8
            or color["primaries"] != "bt709" or conversion is None or decoded["model_input"] != conversion):
        raise ValueError("extension requires canonical SDR model input conversion")
    return provider, integer(decoded["source_frame"], 0, U64_MAX)


def relative_picture(value):
    exact_keys(value, ["position", "picture"])
    return exact_ratio(value["position"]), picture_identity(value["picture"])


def validate_sample(sample, boundary, anchor_position):
    position, (provider, frame) = relative_picture(sample)
    clock = next(iter(boundary.values()))["clock"]
    expected_provider, expected_frame = boundary_identity(boundary)
    # BoundaryPicture cannot independently prove the generated crop recipe.
    comparable = provider[:2] if provider[0] == "generated" else provider
    if (position != exact_ratio(clock["position"]) - anchor_position
            or comparable != expected_provider or frame != expected_frame):
        raise ValueError("input sample differs from its conditioning picture")


def target_rectangle(value):
    exact_keys(value, ["center", "size"])
    result = {}
    for key, low in (("center", 0), ("size", 1)):
        pair = value[key]
        if not isinstance(pair, list) or len(pair) != 2:
            raise ValueError("invalid retained target rectangle")
        result[key] = [integer(item, low, 1_000_000) for item in pair]
    return result


def target_record(value):
    """Validate and serialize in Rust AttentionTarget field order for its hash."""
    required = {"label", "asset", "span", "region"}
    if (not isinstance(value, dict) or not required <= value.keys()
            or value.keys() - required - {"samples", "corrections", "provenance"}):
        raise ValueError("invalid retained target fields")
    _core_identifier(value["asset"], "target asset")
    label = value["label"]
    if not isinstance(label, str) or not label.strip(RUST_WHITESPACE) or len(label.encode("utf-8")) > 128:
        raise ValueError("invalid retained target label")
    exact_keys(value["span"], ["start", "end"])
    start, end = (timestamp(value["span"][key]) for key in ("start", "end"))
    if (start["time_base"] != end["time_base"]
            or not 0 < end["ticks"] - start["ticks"] < (1 << 63)):
        raise ValueError("invalid retained target span")
    canonical = {"label": label, "asset": value["asset"], "span": {"start": start, "end": end},
                 "region": target_rectangle(value["region"])}
    for key, maximum in (("samples", 4096), ("corrections", 256)):
        rows = value.get(key, [])
        if not isinstance(rows, list) or len(rows) > maximum:
            raise ValueError("retained target rows exceed their bound")
        result, previous = [], None
        for row in rows:
            fields = ["at", "region"] + (["confidence", "state"] if key == "samples" else [])
            exact_keys(row, fields)
            at = integer(row["at"], start["ticks"], end["ticks"] - 1)
            if previous is not None and at <= previous:
                raise ValueError("retained target rows must be chronological")
            item = {"at": at, "region": target_rectangle(row["region"])}
            if key == "samples":
                item["confidence"] = integer(row["confidence"], 0, 1000)
                if row["state"] not in ("tracked", "interpolated", "lost"):
                    raise ValueError("invalid retained target track state")
                item["state"] = row["state"]
            result.append(item)
            previous = at
        if result:
            canonical[key] = result
    provenance = value.get("provenance")
    if provenance is not None:
        exact_keys(provenance, ["rule", "engine", "stop"])
        engine = provenance["engine"]
        if (provenance["rule"] != "deadpan-track-1"
                or provenance["stop"] not in ("range_end", "shot_boundary", "picture_limit")
                or not isinstance(engine, str) or not 1 <= len(engine.encode("utf-8")) <= 96
                or any(ord(c) < 32 or 127 <= ord(c) <= 159 or c in "\u2028\u2029" for c in engine)):
            raise ValueError("invalid retained target provenance")
        canonical["provenance"] = {key: provenance[key] for key in ("rule", "engine", "stop")}
    return canonical


def validate_region(binding, captured):
    selected = binding["region"]
    if captured["selection"] == "none":
        if selected is not None:
            raise ValueError("input binding has a different region selection")
        return
    exact_keys(selected, ["id", "record"])
    _core_identifier(selected["id"], "selected target")
    record = target_record(selected["record"])
    encoded = json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if (selected["id"] != captured["target"] or record["label"] != captured["label"]
            or hashlib.sha256(encoded).hexdigest() != captured["target_sha256"]):
        raise ValueError("retained target differs from captured region identity")
    # The Rust host reconstructs captured anchor geometry from this exact record.


def validate_binding(context):
    binding = context["continuity"]["binding"]
    exact_keys(binding, ["duration", "frame_rate", "canvas", "inputs", "region"])
    if len(json.dumps(binding, ensure_ascii=False, separators=(",", ":")).encode("utf-8")) > 512 * 1024:
        raise ValueError("input binding exceeds its byte limit")
    sampling = context["plan"]["sampling"]
    if (integer(binding["duration"], 1, (1 << 63) - 1) != sampling["output_frame_count"]
            or positive_rate(binding["frame_rate"]) != positive_rate(sampling["project_rate"])):
        raise ValueError("input binding differs from the extension plan")
    canvas = binding["canvas"]
    if not isinstance(canvas, list) or len(canvas) != 2:
        raise ValueError("invalid input canvas")
    aw, ah = (integer(side, 1, 65536) for side in canvas)
    native = context["plan"]["native_dimensions"]
    width, height = native["width"], native["height"]
    half_up = lambda n, d, bound: min(bound, max(1, (2 * n + d) // (2 * d)))
    fw, fh = ((half_up(height * aw, ah, width), height) if width * ah > height * aw
              else (width, half_up(width * ah, aw, height)))
    if context["presentation"] != {"x": (width - fw) // 2, "y": (height - fh) // 2, "width": fw, "height": fh}:
        raise ValueError("input canvas differs from the presentation rectangle")
    inputs = binding["inputs"]
    exact_keys(inputs, ["operation", "capture", "samples", "opposite", "support", "terminal"])
    capture = inputs["capture"]
    exact_keys(capture, ["operation", "direction", "native_rate", "context_frames", "policy"])
    if (inputs["operation"] != "extension" or capture["operation"] != "extension"
            or capture["direction"] != sampling["direction"] or capture["policy"] != "temporal_context_v1"
            or positive_rate(capture["native_rate"]) != positive_rate(sampling["native_rate"])
            or integer(capture["context_frames"], 1, 64) != sampling["context_frame_count"]):
        raise ValueError("input capture differs from the extension plan")
    samples, entries = inputs["samples"], context["context"]
    if not isinstance(samples, list) or len(samples) != len(entries):
        raise ValueError("input samples differ from the conditioning pictures")
    anchor = entries[-1] if sampling["direction"] == "from_left" else entries[0]
    anchor_position = exact_ratio(next(iter(anchor["picture"].values()))["clock"]["position"])
    for sample, entry in zip(samples, entries):
        validate_sample(sample, entry["picture"], anchor_position)
    opposite = context["opposite"]
    if opposite["status"] == "absent":
        if inputs["opposite"] is not None:
            raise ValueError("input binding has a different opposite seam")
    else:
        validate_sample(inputs["opposite"], opposite["picture"], anchor_position)
    terminal = relative_picture(inputs["terminal"])
    points = [relative_picture(sample) for sample in samples]
    if terminal != points[-1] or any(a[0] >= b[0] for a, b in zip(points, points[1:])):
        raise ValueError("input terminal or chronological order differs")
    support = inputs["support"]
    if not isinstance(support, list) or len(support) >= 8192:
        raise ValueError("input support exceeds its bound")
    next_position = points[0][0]
    intervals = []
    for span in support:
        exact_keys(span, ["start", "end_exclusive", "first", "last"])
        start, end = exact_ratio(span["start"]), exact_ratio(span["end_exclusive"])
        first, last = picture_identity(span["first"]), picture_identity(span["last"])
        if start != next_position or not start < end <= terminal[0] or first[0] != last[0]:
            raise ValueError("input support must cover context without gaps, overlap or provider changes")
        for position, identity in points:
            if start <= position < end:
                if ((position == start and identity != first) or identity[0] != first[0]
                        or (identity[1] is not None and not min(first[1], last[1]) <= identity[1] <= max(first[1], last[1]))):
                    raise ValueError("input sample differs from its support span")
        intervals.append((first, last))
        next_position = end
    if next_position != terminal[0]:
        raise ValueError("input support does not reach its terminal")
    intervals.append((terminal[1], terminal[1]))
    validate_region(binding, context["region"])
    return intervals


def validate_continuity(context, manifest_reference):
    evidence = context["continuity"]
    exact_keys(evidence, ["capture_policy", "shot_rule", "signature_encoding", "binding",
                          "pictures", "source_picture_counts", "signatures"])
    if (evidence["capture_policy"] != "deadpan-extension-context-1"
            or evidence["shot_rule"] != "deadpan-context-shots-1"
            or evidence["signature_encoding"] != "deadpan-context-signatures-1"):
        raise ValueError("unsupported extension continuity evidence")
    pictures, counts = evidence["pictures"], evidence["source_picture_counts"]
    if not isinstance(pictures, list) or len(pictures) > MAX_SIGNATURES:
        raise ValueError("signature identities exceed their bound")
    identities = [picture_identity(picture) for picture in pictures]
    if any(frame is None for _, frame in identities) or len(set(identities)) != len(identities):
        raise ValueError("signature identities must be unique decoded pictures")
    intervals = validate_binding(context)
    if not isinstance(counts, list) or len(counts) > 8192 or len(counts) != len(intervals):
        raise ValueError("source counts differ from support intervals")
    known_counts, required, decoded = {}, set(), 0
    for ((provider, first), (_, last)), count in zip(intervals, counts):
        if first is None:
            if count is not None:
                raise ValueError("authored black cannot declare a source count")
            continue
        count = integer(count, 1, U64_MAX)
        if max(first, last) >= count or known_counts.get(provider, count) != count:
            raise ValueError("support differs from its physical source count")
        known_counts[provider] = count
        decoded += 1
        begin, end = max(0, min(first, last) - CONTEXT_PADDING), min(count, max(first, last) + 1 + CONTEXT_PADDING)
        if end - begin > MAX_SIGNATURES or decoded + len(identities) > MAX_SIGNATURES:
            raise ValueError("continuity exceeds its total picture-read budget")
        required.update((provider, ordinal) for ordinal in range(begin, end))
        if len(required) > MAX_SIGNATURES:
            raise ValueError("continuity exceeds its signature budget")
    if required != set(identities):
        raise ValueError("continuity signatures have missing or unrelated physical pictures")
    artifact = evidence["signatures"]
    exact_keys(artifact, ["reference", "sha256", "byte_length"])
    WorkspaceArtifact(**artifact)
    if artifact["byte_length"] != 12 + SIGNATURE_BYTES * len(pictures) or artifact["byte_length"] > MAX_SIGNATURE_BYTES:
        raise ValueError("invalid continuity signature byte length")
    frames = [entry["frame"] for entry in context["context"]]
    if context["opposite"]["status"] == "present_unconditioned":
        frames.append(context["opposite"]["frame"])
    if artifact["reference"] == manifest_reference:
        raise ValueError("extension manifest and signatures must not alias")
    known_frames = {}
    for frame in frames:
        reference = frame["reference"]
        if reference in (manifest_reference, artifact["reference"]):
            raise ValueError("extension PNGs must not alias manifest or signatures")
        if reference in known_frames and known_frames[reference] != frame:
            raise ValueError("repeated extension PNG has contradictory artifact declarations")
        known_frames[reference] = frame


def validate_signature_bytes(evidence, data):
    artifact = evidence["signatures"]
    count = len(evidence["pictures"])
    if (len(data) != artifact["byte_length"] or len(data) != 12 + SIGNATURE_BYTES * count
            or len(data) > MAX_SIGNATURE_BYTES or hashlib.sha256(data).hexdigest() != artifact["sha256"]):
        raise ValueError("continuity signature hash or length mismatch")
    if data[:8] != b"DPSIG001" or struct.unpack_from("<I", data, 8)[0] != count:
        raise ValueError("invalid continuity signature header or count")
    for index in range(count):
        bins = struct.unpack_from("<32I", data, 12 + index * SIGNATURE_BYTES + 1728)
        if any(value > 65536 for value in bins) or not 65520 <= sum(bins) <= 65552:
            raise ValueError("invalid continuity signature histogram")
