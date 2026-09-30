"""Transport fixtures only: replay captured bytes or emit hostile verifier claims.

The encoder branch asserts the full captured clock contract, then deliberately
rebinds document identity to the test's background project. It never encodes or
checks whether those pictures/samples match that project. The verifier branch
never decodes media and must only be used to test host protocol handling.
"""

import copy
import hashlib
import json
from pathlib import Path
import struct
import sys


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
    sys.stdout.buffer.write(struct.pack(">I", len(payload)) + payload)
    sys.stdout.buffer.flush()


def emit(message):
    emit_bytes(json.dumps(message, separators=(",", ":")).encode("utf-8"))


request = read_message()
if sys.argv[1] == "owned-wait":
    # Exit only after host cancellation, proving revocation reaches supervision.
    assert request["op"] in ["prepare", "inspect"]
    if request["op"] == "prepare":
        picture = request["contract"]["picture"]
        emit({"event": "progress", "protocol": 3, "identity": request["identity"],
              "completed_frames": 0, "total_frames": picture["frame_count"],
              "completed_audio_samples": 0,
              "total_audio_samples": picture["project_audio_end"] - picture["project_audio_start"]})
    else:
        manifest = request["manifest"]
        emit({"event": "progress", "protocol": 1, "identity": request["identity"],
              "progress": {"stage": "packets", "completed": 0,
                           "total": manifest["report"]["video_packets"] + manifest["report"]["audio_packets"]}})
    cancel = read_message()
    assert cancel["op"] == "cancel" and cancel["identity"] == request["identity"]
    assert cancel["cancellation_token"] == request["cancellation_token"]
    Path(sys.argv[2]).write_text("host cancelled revoked owner\n")
    emit({"event": "cancelled", "protocol": request["protocol"], "identity": request["identity"]})
    raise SystemExit(0)

if sys.argv[1] == "encode":
    assert sys.argv[4] == "--render-encode-worker"
    assert Path(sys.argv[5]).is_absolute()
    assert request["op"] == "prepare"
    assert request["protocol"] == 3 and request["binding"] is None
    manifest = json.loads(Path(sys.argv[2]).read_text())
    assert manifest["contract"] == request["contract"], "captured contract changed"
    assert manifest["report"]["info"]["maximum_moov_bytes"] == (
        request["limits"]["maximum_packets"] * 128 + 1048576
    )
    payload = Path(sys.argv[3]).read_bytes()
    Path("output/movie.mp4").write_bytes(payload)
    manifest["document_sha256"] = request["document_sha256"]
    manifest["movie"]["sha256"] = hashlib.sha256(payload).hexdigest()
    manifest["movie"]["byte_length"] = len(payload)
    manifest["report"]["output_bytes"] = len(payload)
    emit({"event": "completed", "protocol": 3, "identity": request["identity"],
          "manifest": manifest, "binding": None})
    raise SystemExit(0)

assert sys.argv[1] == "verify"
mode = sys.argv[2]
if mode == "completed_exit_failure":
    witness = Path(sys.argv[3])
    assert witness.is_absolute()
    assert sys.argv[4] == "--render-verify-worker"
else:
    assert sys.argv[3] == "--render-verify-worker"
assert request["op"] == "inspect"
assert Path("input/movie.mp4").is_file()
manifest = request["manifest"]
identity = request["identity"]
encoded = manifest["report"]
frames = encoded["video_frames"]
samples = encoded["audio_samples"]
total_packets = encoded["video_packets"] + encoded["audio_packets"]
report = {
    "policy_version": 1,
    "contract": copy.deepcopy(manifest["contract"]),
    "document_sha256": manifest["document_sha256"],
    "movie_sha256": manifest["movie"]["sha256"],
    "movie_bytes": manifest["movie"]["byte_length"],
    "video_frames": frames, "audio_samples": samples,
    "video_packets": encoded["video_packets"], "audio_packets": encoded["audio_packets"],
    "gops": 1, "fresh_gop_frames": frames, "maximum_b_run": 0,
    "runtime_versions": [4066151, 4064103, 3934311],
    "movie_timescale": encoded["info"]["movie_timescale"],
    "video_edit_media_time": 0, "audio_edit_media_time": 1024,
    "manual_first_sample": -1024,
    "manual_physical_samples": encoded["audio_packets"] * 1024,
    "ordinary_first_sample": 0,
    "ordinary_physical_samples": ((samples + 1023) // 1024) * 1024,
}


def progress(stage, completed, total):
    emit({"event": "progress", "protocol": 1, "identity": identity,
          "progress": {"stage": stage, "completed": completed, "total": total}})


if mode == "malformed":
    emit_bytes(b"{not-json}")
    raise SystemExit(0)
if mode == "partial_body":
    sys.stdout.buffer.write(struct.pack(">I", 100) + b"{")
    sys.stdout.buffer.flush()
    raise SystemExit(0)
if mode == "failed_exit":
    emit({"event": "failed", "protocol": 1, "identity": identity,
          "diagnostic": "fixture verifier rejected captured bytes"})
    raise SystemExit(1)
if mode == "stage_regression":
    progress("pictures", 0, frames)
    progress("packets", 1, total_packets)
elif mode == "count_regression":
    progress("packets", 2, total_packets)
    progress("packets", 1, total_packets)
elif mode == "wrong_total":
    progress("packets", 1, total_packets + 1)
elif mode == "wrong_stage":
    progress("future_stage", 0, 1)
elif mode == "over_progress":
    progress("pictures", frames + 1, frames)
elif mode == "inconsistent_report":
    report["fresh_gop_frames"] = frames + 1
elif mode == "wrong_movie":
    report["movie_sha256"] = "0" * 64
elif mode == "wrong_document":
    report["document_sha256"] = "0" * 64
elif mode == "wrong_runtime":
    report["runtime_versions"][0] = 0

completed = {"event": "completed", "protocol": 1, "identity": copy.deepcopy(identity),
             "report": report}
if mode == "stale_attempt":
    completed["identity"]["attempt_id"] = "foreign-attempt"
emit(completed)
if mode == "after_terminal":
    progress("packets", total_packets, total_packets)
elif mode == "duplicate_terminal":
    emit(completed)
elif mode == "completed_exit_failure":
    with witness.open("x") as output:
        json.dump(completed, output)
    raise SystemExit(1)
