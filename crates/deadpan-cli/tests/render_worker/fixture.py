"""Host-admission fault fixture. This does not render or qualify GPU pixels."""

import hashlib
import json
from pathlib import Path
import runpy
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


def emit_bytes(payload):
    sys.stdout.buffer.write(struct.pack(">I", len(payload)))
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()


def emit(message):
    emit_bytes(json.dumps(message, separators=(",", ":")).encode("utf-8"))


mode = sys.argv[1]
assert sys.argv[2] == "--render-picture-worker"
assert Path(sys.argv[3]).is_absolute()
length = struct.unpack(">I", read_exact(4))[0]
assert 0 < length <= 256 * 1024
request = json.loads(read_exact(length))
assert request["op"] == "prepare"
assert request["contract"]["raster"] == [2, 2]
assert request["contract"]["frame_count"] == 2
identity = request["identity"]

# Hostile modes (tests/hostile_workers): misbehave, or claim a hostile artifact.
hostile = runpy.run_path(str(Path(__file__).parent.parent / "hostile_workers" / "hostile.py"))
hostile["generic"](mode)
claim = hostile["parse"](mode)
if claim is not None:
    planes = bytes([16, 16, 16, 16, 128, 128] * 2)
    (claim[1] / "outside.bin").write_bytes(planes)
    assert hostile["artifact"](mode, "output/pictures.i420", planes) is not None, mode
    if claim[0] == "wrong_attempt":
        identity = dict(identity, attempt_id="another-attempt")
    emit({
        "event": "completed", "protocol": 1, "identity": identity,
        "manifest": {
            "contract": request["contract"],
            "document_sha256": request["document_sha256"],
            "planes": {"reference": hostile["reference"](mode, "output/pictures.i420"),
                       "sha256": hashlib.sha256(planes).hexdigest(), "byte_length": 12},
            "pixel_policy": "i420_rec709_limited_left",
        },
    })
    raise SystemExit(0)

if mode == "failed_exit":
    emit({"event": "failed", "protocol": 1, "identity": identity,
          "diagnostic": "fixture decoder rejected captured source"})
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

admitted_modes = {
    "valid", "wrong_hash", "short_file", "invalid_luma", "invalid_chroma",
    "stale_attempt", "completed_exit_failure", "after_terminal",
}
assert mode in admitted_modes
planes = bytearray([16, 16, 16, 16, 128, 128] * 2)
if mode == "short_file":
    del planes[-1]
if mode == "invalid_luma":
    planes[0] = 0
if mode == "invalid_chroma":
    planes[4] = 255
with open("output/pictures.i420", "xb") as output:
    output.write(planes)
digest = hashlib.sha256(planes).hexdigest()
if mode == "wrong_hash":
    digest = "0" * 64
if mode == "stale_attempt":
    identity = dict(identity, attempt_id="previous-attempt")
emit({
    "event": "completed", "protocol": 1, "identity": identity,
    "manifest": {
        "contract": request["contract"],
        "document_sha256": request["document_sha256"],
        "planes": {"reference": "output/pictures.i420", "sha256": digest,
                   "byte_length": 12},
        "pixel_policy": "i420_rec709_limited_left",
    },
})
if mode == "completed_exit_failure":
    raise SystemExit(1)
if mode == "after_terminal":
    emit({"event": "progress", "protocol": 1, "identity": identity,
          "completed_frames": 2, "total_frames": 2})
