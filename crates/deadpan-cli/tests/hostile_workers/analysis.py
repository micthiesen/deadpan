"""Hostile stand-in for the tracking, face, transcription and AI pause workers.

The test points each host's trusted executable seam at a wrapper that runs
this file with a `hostile:<name>:<record directory>` mode. It reads the
host's first framed request, then either misbehaves without regard to the
protocol (hostile.py) or answers with a valid-looking completion whose
claimed artifact is hostile. It never decodes media or runs a model.
"""

import hashlib
import json
from pathlib import Path
import runpy
import struct
import sys

hostile = runpy.run_path(str(Path(__file__).with_name("hostile.py")))


def read_exact(length):
    result = bytearray()
    while len(result) < length:
        chunk = sys.stdin.buffer.read(length - len(result))
        if not chunk:
            raise RuntimeError("host request ended early")
        result.extend(chunk)
    return bytes(result)


def emit(message):
    payload = json.dumps(message, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(struct.pack(">I", len(payload)) + payload)
    sys.stdout.buffer.flush()


mode = sys.argv[1]
length = struct.unpack(">I", read_exact(4))[0]
assert 0 < length <= 256 * 1024
request = json.loads(read_exact(length))
hostile["generic"](mode)
name, directory = hostile["parse"](mode)


def place(path, payload):
    """Create the claimed artifact, hostile or not, and describe it."""
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    (directory / "outside.bin").write_bytes(payload)
    declared = hostile["artifact"](mode, path, payload)
    assert declared is not None, mode
    return {"reference": hostile["reference"](mode, path),
            "sha256": hashlib.sha256(declared).hexdigest(),
            "byte_length": len(declared)}


if request.get("op") == "track":
    identity = {"protocol": 1, "request": request["request"],
                "attempt": request["attempt"]}
    if name == "wrong_attempt":
        identity["attempt"] = "another-attempt"
    pictures = json.loads((directory / "pictures.json").read_text())
    stride = request["stride"]
    payload = json.dumps({
        "decoded": pictures,
        "observations": [{"pts": pts, "region": request["region"], "confidence": 1.0}
                          for pts in pictures[::stride]],
    }).encode("utf-8")
    emit(dict(identity, event="completed",
              observations=place(request["output_scope"] + "/track.json", payload),
              runtime={"engine": "hostile-fixture", "request_revision": 1,
                       "tracking_level": "accurate"},
              decoded=len(pictures), analysed=len(pictures[::stride]),
              decode_millis=1, vision_millis=1, elapsed_millis=1))
elif request.get("op") == "transcribe":
    identity = {"protocol": 2, "request": request["request"],
                "attempt": request["attempt"]}
    if name == "wrong_attempt":
        identity["attempt"] = "another-attempt"
    emit(dict(identity, event="completed",
              transcript=place(request["output_scope"] + "/transcript.json", b"[]"),
              runtime={"engine": "hostile-fixture", "backend": "cpu",
                       "model_sha256": request["model"]["sha256"]},
              elapsed_millis=1))
elif request.get("op") == "detect_faces":
    if name == "wrong_attempt":
        emit({"event": "failed", "protocol": 1, "request": request["request"],
              "attempt": "another-attempt", "diagnostic": "not this attempt"})
elif request.get("operation") == "generate_bridge":
    if name == "wrong_attempt":
        identity = dict(request["identity"], attempt_id="another-attempt")
        emit({"event": "stage", "protocol": 2, "identity": identity,
              "stage": "preflight"})
    else:
        for stage in ["preflight", "inference"]:
            emit({"event": "stage", "protocol": 2, "identity": request["identity"],
                  "stage": stage})
        scope = request["output_workspace"]
        # Provenance is checked before decoding native output. A valid-looking
        # declaration reaches that production containment gate without asking
        # the media worker to interpret these deliberately non-media bytes.
        provenance = place(scope + "/provenance.json", b"{}")
        native = request["plan"]["native"]
        emit({"event": "completed_bridge", "protocol": 2,
              "identity": request["identity"], "candidate": {
                  "native": {"reference": scope + "/native.mp4",
                             "sha256": hashlib.sha256(b"not media").hexdigest(),
                             "byte_length": 9},
                  "provenance": provenance,
                  "video": {"frames": native["frame_count"],
                            "frame_rate": native["frame_rate"],
                            "width": native["width"], "height": native["height"]},
                  "provider": request["provider"],
              }})
else:
    raise SystemExit("unexpected request " + json.dumps(request)[:200])
