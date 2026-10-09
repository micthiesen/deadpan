"""Host transport/admission fixtures. These bytes are deliberately not media."""

import copy
import hashlib
import json
import math
import os
from pathlib import Path
import runpy
import struct
import sys
import time


def read_exact(length):
    result = bytearray()
    while len(result) < length:
        chunk = sys.stdin.buffer.read(length - len(result))
        if not chunk:
            raise RuntimeError("host request ended early")
        result.extend(chunk)
    return bytes(result)


def read_message():
    length = struct.unpack(">I", read_exact(4))[0]
    assert 0 < length <= 256 * 1024
    return json.loads(read_exact(length))


def emit_bytes(payload):
    sys.stdout.buffer.write(struct.pack(">I", len(payload)))
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()


def emit(message):
    emit_bytes(json.dumps(message, separators=(",", ":")).encode("utf-8"))


mode = sys.argv[1]
assert sys.argv[2] == "--render-encode-worker"
assert Path(sys.argv[3]).is_absolute()
request = read_message()
assert request["op"] == "prepare"
assert request["protocol"] == 3
assert request["binding"] is None
protocol = request["protocol"]
picture = request["contract"]["picture"]
assert picture["raster"] == [2, 2]
identity = request["identity"]
video_frames = picture["frame_count"]
audio_samples = picture["project_audio_end"] - picture["project_audio_start"]
payload = b"not an MP4: host-admission fixture\n"
digest = hashlib.sha256(payload).hexdigest()
# Hostile modes (tests/hostile_workers): misbehave, or claim a hostile artifact.
hostile = runpy.run_path(str(Path(__file__).parent.parent / "hostile_workers" / "hostile.py"))
hostile["generic"](mode)
claim = hostile["parse"](mode)


def progress(frames, samples):
    emit({"event": "progress", "protocol": protocol, "identity": identity,
          "completed_frames": frames, "total_frames": video_frames,
          "completed_audio_samples": samples, "total_audio_samples": audio_samples})


# Invalid protocol/teardown cases leave a FIFO: if host admission incorrectly
# attempts a snapshot, it produces an Artifact error instead of the expected
# protocol/process outcome. The file is never a successful private candidate.
artifact_modes = {"valid", "wrong_hash", "short_file", "long_file", "symlink", "hardlink"}
movie = Path("output/movie.mp4")
if claim is not None:
    (claim[1] / "outside.bin").write_bytes(payload)
    assert hostile["artifact"](mode, movie, payload) is not None, mode
elif mode in artifact_modes:
    if mode == "symlink":
        target = Path("output/other.mp4")
        target.write_bytes(payload)
        movie.symlink_to("other.mp4")
    elif mode == "hardlink":
        target = Path("output/other.mp4")
        target.write_bytes(payload)
        os.link(target, movie)
    else:
        movie.write_bytes(payload[:-1] if mode == "short_file" else
                          payload + b"extra" if mode == "long_file" else payload)
else:
    os.mkfifo(movie)

if mode == "failed_exit":
    emit({"event": "failed", "protocol": protocol, "identity": identity,
          "failure": {"kind": {"stage": "encoder", "kind": "input"},
                      "diagnostic": "fixture encoder rejected captured input"}})
    raise SystemExit(1)
if mode.startswith("failure:"):
    kind = mode.split(":", 1)[1]
    failure = {"kind": {"stage": "encoder", "kind": kind},
               "diagnostic": "fixture typed encoder rejection"}
    if kind == "source":
        failure = {"kind": {"stage": "source"},
                   "diagnostic": "video_encoder_unavailable: misleading source error"}
    message = {"event": "failed", "protocol": protocol, "identity": identity,
               "failure": failure}
    if kind == "legacy":
        message = {"event": "failed", "protocol": 1, "identity": identity,
                   "diagnostic": "old worker has no failure kind"}
    elif kind == "stale":
        message["identity"] = dict(identity, attempt_id="previous-attempt")
        failure["kind"]["kind"] = "encoder_unavailable"
    elif kind == "unknown_field":
        failure["kind"]["kind"] = "encoder_unavailable"
        failure["fallback"] = True
    elif kind in {"then_malformed", "then_duplicate", "then_crash", "then_exit2", "then_hang"}:
        failure["kind"]["kind"] = "encoder_unavailable"
    emit(message)
    if kind == "then_malformed":
        emit_bytes(b"{malformed after failure}")
    elif kind == "then_duplicate":
        emit(message)
    elif kind == "then_crash":
        import signal
        os.kill(os.getpid(), signal.SIGKILL)
    elif kind == "then_exit2":
        raise SystemExit(2)
    elif kind == "then_hang":
        while True:
            time.sleep(10)
    raise SystemExit(1)
if mode == "partial_header":
    sys.stdout.buffer.write(b"\x00\x00")
    sys.stdout.buffer.flush()
    raise SystemExit(0)
if mode == "partial_body":
    sys.stdout.buffer.write(struct.pack(">I", 1024) + b"{")
    sys.stdout.buffer.flush()
    raise SystemExit(0)
if mode == "malformed":
    emit_bytes(b"{not-json}")
    raise SystemExit(0)
if mode == "oversized":
    sys.stdout.buffer.write(struct.pack(">I", 256 * 1024 + 1))
    sys.stdout.buffer.flush()
    raise SystemExit(0)
if mode in {"cancel", "ignore_cancel"}:
    progress(1, 512)
    if mode == "ignore_cancel":
        while True:
            time.sleep(10)
    cancel = read_message()
    assert cancel["op"] == "cancel"
    assert cancel["identity"] == identity
    assert cancel["cancellation_token"] == request["cancellation_token"]
    emit({"event": "cancelled", "protocol": protocol, "identity": identity})
    raise SystemExit(0)
if mode in {"video_regression", "audio_regression"}:
    progress(1, 512)
    progress(0 if mode == "video_regression" else 1,
             256 if mode == "audio_regression" else 1024)
    # Continue to a valid completion. Without the progress rejection, the host
    # reaches the FIFO below, so a missing-terminal error cannot mask the bug.
if mode in {"wrong_video_total", "wrong_audio_total", "over_video", "over_audio"}:
    message = {"event": "progress", "protocol": protocol, "identity": identity,
               "completed_frames": 1, "total_frames": video_frames,
               "completed_audio_samples": 512, "total_audio_samples": audio_samples}
    key = {"wrong_video_total": "total_frames", "wrong_audio_total": "total_audio_samples",
           "over_video": "completed_frames", "over_audio": "completed_audio_samples"}[mode]
    message[key] = (video_frames if "video" in mode else audio_samples) + 1
    emit(message)

rate = picture["frame_rate"]
numerator = rate["numerator"]
denominator = rate["denominator"]
choice = request["contract"]["choice"]
b_frames = 0 if choice["b_frames"] == "none" else 2
video_packets = video_frames
audio_packets = (audio_samples + 1023) // 1024 + 2
report = {
    "info": {
        "abi_version": 2, "avcodec_version": 4066151, "avformat_version": 4064103,
        "avutil_version": 3934311, "movie_timescale": math.lcm(numerator, 48000),
        "video_time_base_num": 1, "video_time_base_den": numerator,
        "audio_time_base_num": 1, "audio_time_base_den": 48000,
        "audio_frame_size": 1024, "video_profile": 100,
        "video_has_b_frames": b_frames, "video_max_b_frames": b_frames,
        "video_gop_size": max(1, (numerator + denominator) // (2 * denominator)),
        "audio_profile": 1, "audio_initial_padding": 1024, "audio_trailing_padding": 0,
        "requested_mode": choice["mode"], "video_bitrate": 1500000,
        "audio_bitrate": 384000,
        "maximum_moov_bytes": request["limits"]["maximum_packets"] * 128 + 1048576,
    },
    "video_frames": video_frames, "audio_samples": audio_samples,
    "video_packets": video_packets, "audio_packets": audio_packets,
    "output_bytes": len(payload), "packet_bytes": video_packets + audio_packets,
    "video_duration_from_contract_packets": video_frames,
    "faststart_read_opens": 1, "faststart_read_closes": 1,
    "video_eof": True, "audio_eof": True,
}
manifest = {"contract": copy.deepcopy(request["contract"]),
            "document_sha256": request["document_sha256"],
            "movie": {"reference": "output/movie.mp4", "sha256": digest,
                      "byte_length": len(payload)}, "report": report}
manifest["movie"]["reference"] = hostile["reference"](mode, "output/movie.mp4")
if claim is not None and claim[0] == "wrong_attempt":
    identity = dict(identity, attempt_id="another-attempt")
if mode == "wrong_hash":
    manifest["movie"]["sha256"] = "0" * 64
elif mode == "wrong_document":
    manifest["document_sha256"] = "0" * 64
elif mode == "wrong_movie_path":
    manifest["movie"]["reference"] = "output/other.mp4"
elif mode == "outside_scope":
    manifest["movie"]["reference"] = "other/movie.mp4"
elif mode.startswith("contract:"):
    field = mode.split(":", 1)[1]
    changed = manifest["contract"]["picture"]
    replacements = {
        "project_id": "other-project", "revision_id": "other-revision",
        "range": {"start": 0, "end": 2}, "canvas": [4, 2], "raster": [4, 2],
        "frame_rate": {"numerator": 30, "denominator": 1}, "color_policy": "hdr_rec2020_pq",
        "time_base": {"numerator": 1, "denominator": 30}, "frame_count": 1,
        "terminal_pts": 1001, "project_audio_start": 0, "project_audio_end": 9999,
        "relative_aspect_error": {"numerator": 1, "denominator": 10},
    }
    changed[field] = replacements[field]
elif mode == "wrong_mode":
    manifest["contract"]["choice"]["mode"] = "software"
elif mode == "wrong_b_frames":
    manifest["contract"]["choice"]["b_frames"] = "none"
elif mode.startswith("report:"):
    field = mode.split(":", 1)[1]
    replacements = {
        "video_frames": video_frames - 1, "audio_samples": audio_samples + 1,
        "video_packets": video_packets + 1, "audio_packets": audio_packets - 1,
        "output_bytes": len(payload) + 1, "packet_bytes": 0,
        "video_duration_from_contract_packets": video_frames + 1,
        "faststart_read_opens": 2, "faststart_read_closes": 0,
        "video_eof": False, "audio_eof": False,
    }
    report[field] = replacements[field]
elif mode.startswith("info:"):
    field = mode.split(":", 1)[1]
    replacements = {
        "abi_version": 1, "avcodec_version": 0, "movie_timescale": 30000,
        "video_time_base_num": 2, "video_time_base_den": 30,
        "audio_time_base_num": 2, "audio_time_base_den": 44100,
        "audio_frame_size": 512, "video_profile": 77, "audio_profile": 0,
        "video_max_b_frames": 0, "video_has_b_frames": 3, "video_gop_size": 30,
        "audio_initial_padding": -1, "audio_trailing_padding": 8193,
        "requested_mode": "software", "video_bitrate": 3000000,
        "audio_bitrate": 128000, "maximum_moov_bytes": 1048576,
    }
    report["info"][field] = replacements[field]

if mode == "stale_attempt":
    identity = dict(identity, attempt_id="previous-attempt")
elif mode == "wrong_request":
    identity = dict(identity, request_id="other-request")
emit({"event": "completed", "protocol": 1 if mode == "wrong_version" else protocol,
      "identity": identity, "manifest": manifest, "binding": None})
if mode == "completed_exit_failure":
    raise SystemExit(1)
if mode == "after_terminal":
    progress(video_frames, audio_samples)
if mode == "duplicate_terminal":
    emit({"event": "completed", "protocol": protocol, "identity": identity, "manifest": manifest, "binding": None})
