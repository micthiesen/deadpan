import os
import signal
import struct
import sys
import time
from pathlib import Path


def read():
    count = struct.unpack(">I", sys.stdin.buffer.read(4))[0]
    return json.loads(sys.stdin.buffer.read(count))


def emit(value):
    data = json.dumps(value, separators=(",", ":")).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(data)) + data)
    sys.stdout.buffer.flush()


assert sys.argv[1:] == ["--render-probe-worker"]
request = read()
assert request["op"] == "probe"
assert request["protocol"] == 1
with Path(CONFIG["trace"]).open("a") as trace:
    trace.write(json.dumps(request) + "\n")
mode = CONFIG["mode"]
choice = request["spec"]["choice"]
identity = request["identity"]
if mode == "regressed_progress":
    for count in [2, 1]:
        emit({"event": "progress", "protocol": 1, "identity": identity,
              "completed_frames": count, "total_frames": 46})
    cancellation = read()
    assert cancellation["op"] == "cancel"
    assert cancellation["identity"] == identity
    assert cancellation["cancellation_token"] == request["cancellation_token"]
    with Path(CONFIG["trace"]).with_suffix(".cancelled").open("w") as receipt:
        receipt.write("matching cancellation received")
    emit({"event": "cancelled", "protocol": 1, "identity": identity})
    raise SystemExit(0)
kind = {"stage": "encoder", "kind": "video_timestamp_order"}
if mode == "sequence" and choice["b_frames"] == "none":
    kind = {"stage": "output"}
elif mode == "unavailable":
    kind = ({"stage": "encoder", "kind": "encoder_unavailable"}
            if choice["mode"] == "hardware" else {"stage": "audio"})
elif mode == "capacity":
    kind = {"stage": "encoder", "kind": "capacity"}
elif mode == "misleading":
    kind = {"stage": "source"}
elif mode == "wait":
    emit({"event": "progress", "protocol": 1, "identity": identity,
          "completed_frames": 0, "total_frames": 46})
    cancellation = read()
    assert cancellation["op"] == "cancel"
    assert cancellation["identity"] == identity
    assert cancellation["cancellation_token"] == request["cancellation_token"]
    emit({"event": "cancelled", "protocol": 1, "identity": identity})
    raise SystemExit(0)
message = {"event": "failed", "protocol": 1, "identity": identity,
           "failure": {"kind": kind,
                       "diagnostic": "video_timestamp_order: diagnostic cannot select policy"}}
if mode == "stale":
    message["identity"] = dict(identity, attempt_id="stale")
elif mode == "unknown":
    message["failure"]["allow_fallback"] = True
emit(message)
if mode == "malformed_tail":
    sys.stdout.buffer.write(struct.pack(">I", 1) + b"!")
    sys.stdout.buffer.flush()
elif mode == "duplicate":
    emit(message)
elif mode == "crash":
    os.kill(os.getpid(), signal.SIGKILL)
elif mode == "exit2":
    raise SystemExit(2)
elif mode == "hang":
    while True:
        time.sleep(10)
raise SystemExit(1)
