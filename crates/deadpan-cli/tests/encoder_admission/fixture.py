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
assert request["protocol"] == 2
with Path(CONFIG["trace"]).open("a") as trace:
    trace.write(json.dumps(request) + "\n")
mode = CONFIG["mode"]
choice = request["spec"]["choice"]
identity = request["identity"]
host_runtime = request["expected_runtime"]
runtime = dict(schema_version=1,
               platform=dict(os_build="25F84", hardware_model="TestMac1,1", cpu_family=1),
               **{key: host_runtime[key] for key in ["system", "kernel_release", "kernel_build", "machine"]})
runtime["images"] = []
for index, role in enumerate(["helper", "avcodec", "avformat", "avutil", "swscale"]):
    stamp = dict(seconds=1, nanoseconds=0)
    runtime["images"].append(dict(
        mapped=dict(kind=role, device=1, inode=index + 1, uuid=[index + 1] * 16,
                    file_size=host_runtime["helper_bytes"] if index == 0 else 128,
                    modification_time=stamp, change_time=stamp, birth_time=stamp, generation=1),
        sha256=host_runtime["helper_sha256"] if index == 0 else "a" * 64))
if mode == "regressed_progress":
    for count in [2, 1]:
        emit({"event": "progress", "protocol": 2, "identity": identity,
              "completed_frames": count, "total_frames": 46})
    cancellation = read()
    assert cancellation["op"] == "cancel"
    assert cancellation["identity"] == identity
    assert cancellation["cancellation_token"] == request["cancellation_token"]
    with Path(CONFIG["trace"]).with_suffix(".cancelled").open("w") as receipt:
        receipt.write("matching cancellation received")
    emit({"event": "cancelled", "protocol": 2, "identity": identity})
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
    emit({"event": "progress", "protocol": 2, "identity": identity,
          "completed_frames": 0, "total_frames": 46})
    cancellation = read()
    assert cancellation["op"] == "cancel"
    assert cancellation["identity"] == identity
    assert cancellation["cancellation_token"] == request["cancellation_token"]
    emit({"event": "cancelled", "protocol": 2, "identity": identity})
    raise SystemExit(0)
message = {"event": "failed", "protocol": 2, "identity": identity,
           "runtime": runtime,
           "failure": {"kind": kind,
                       "diagnostic": "video_timestamp_order: diagnostic cannot select policy"}}
if mode == "stale":
    message["identity"] = dict(identity, attempt_id="stale")
elif mode == "unknown":
    message["failure"]["allow_fallback"] = True
elif mode == "missing_runtime":
    message["runtime"] = None
elif mode == "helper_mismatch":
    message["runtime"]["images"][0]["sha256"] = "b" * 64
elif mode == "runtime_change" and choice["b_frames"] == "none":
    message["runtime"]["images"][1]["sha256"] = "b" * 64
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
