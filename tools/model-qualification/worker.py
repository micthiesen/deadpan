"""One framed, offline MLX qualification attempt. Run only via the Rust host.

The host owns runtime selection, process termination and output authority. This
developer adapter produces an unaccepted candidate, never a project mutation.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import stat
import sys
import threading
import traceback

# -I omits the script directory. Add only our checked-in adapter directory;
# runtime/model/workspace paths are never added to Python's module search path.
sys.path.insert(0, str(Path(__file__).resolve().parent))

ADAPTER_SOURCES = {name: hashlib.sha256((Path(__file__).resolve().parent / name).read_bytes()).hexdigest()
                   for name in ["worker.py", "worker_protocol.py", "worker_media.py", "mlx_backend.py",
                                "runtime_source.py", "ltx-source-manifest.json"]}

from worker_media import exact_keys, validate_plan
from worker_protocol import (
    FrameRate,
    GenerateBridgeRequest,
    NativeCandidateManifest,
    VideoSpec,
    WorkerProtocol,
    WorkspaceArtifact,
)


class Cancelled(Exception):
    pass


def strict_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate JSON field")
            result[key] = value
        return result

    def constant(_):
        raise ValueError("nonfinite JSON value")
    return json.loads(data, object_pairs_hook=pairs, parse_constant=constant)


def validate_bridge_context(context, request, video):
    if not isinstance(request, GenerateBridgeRequest):
        raise ValueError("development worker requires protocol-2 generate_bridge")
    if not isinstance(context, dict) or context.get("plan") != request.plan:
        raise ValueError("request plan differs from context plan")
    return validate_plan(context["plan"], video)


def contained_read(root, reference, maximum):
    parts = reference.split("/")
    if (not reference or reference.startswith("/") or "\\" in reference or "\0" in reference
            or any(part in ("", ".", "..") for part in parts)):
        raise ValueError("unsafe workspace input")
    directory = os.dup(root)
    try:
        for part in parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = child
        descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        try:
            before = os.fstat(descriptor)
            if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid()
                    or before.st_nlink != 1 or not 0 < before.st_size <= maximum):
                raise ValueError("unsafe or oversized workspace input")
            with os.fdopen(descriptor, "rb", closefd=False) as stream:
                data = stream.read(maximum + 1)
            after = os.fstat(descriptor)
            if (len(data) != before.st_size or before.st_mtime_ns != after.st_mtime_ns
                    or before.st_ctime_ns != after.st_ctime_ns or after.st_size != before.st_size):
                raise ValueError("workspace input changed during read")
            return data
        finally:
            os.close(descriptor)
    finally:
        os.close(directory)


def finished_artifact(output, path, reference, maximum, check_cancel):
    expected = output / reference.removeprefix("outputs/")
    if path != expected:
        raise ValueError("worker returned an unexpected output reference")
    check_cancel()
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    descriptor = os.open(path, flags)
    try:
        before = os.fstat(descriptor)
        if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid()
                or before.st_nlink != 1 or not 0 < before.st_size <= maximum):
            raise ValueError("worker output is not a bounded regular file")
        digest = hashlib.sha256()
        length = 0
        with os.fdopen(os.dup(descriptor), "rb") as stream:
            while data := stream.read(1024 * 1024):
                check_cancel()
                length += len(data)
                if length > before.st_size:
                    raise ValueError("worker output grew during hashing")
                digest.update(data)
        after = os.fstat(descriptor)
        if (length != before.st_size or before.st_dev != after.st_dev
                or before.st_ino != after.st_ino or before.st_size != after.st_size
                or before.st_mtime_ns != after.st_mtime_ns
                or before.st_ctime_ns != after.st_ctime_ns
                or before.st_nlink != after.st_nlink):
            raise ValueError("worker output changed during hashing")
        return WorkspaceArtifact(reference, digest.hexdigest(), before.st_size)
    finally:
        os.close(descriptor)


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-config", required=True, type=Path)
    args = parser.parse_args()
    protocol = WorkerProtocol.from_fds()
    # Native extensions and subprocesses inherit stderr for ordinary stdout.
    os.dup2(2, 1)
    request = protocol.read_request()
    errors = []
    finished = threading.Event()

    def read_cancel():
        try:
            if not protocol.read_cancel() and not finished.is_set():
                errors.append("host control pipe closed during generation")
        except Exception as error:
            errors.append(str(error))

    threading.Thread(target=read_cancel, daemon=True, name="deadpan-cancel-reader").start()

    def check_cancel():
        if errors:
            raise ValueError(errors[0])
        if protocol.cancel_requested:
            raise Cancelled()
        if resource.getrusage(resource.RUSAGE_SELF).ru_maxrss > 80 * 1024**3:
            raise MemoryError("process RSS exceeded the development envelope")

    last_stage = None

    def stage(value):
        nonlocal last_stage
        check_cancel()
        if value != last_stage:
            protocol.emit_stage(value)
            last_stage = value

    try:
        stage("preflight")
        if not isinstance(request, GenerateBridgeRequest):
            raise ValueError("development worker requires protocol-2 generate_bridge")
        if sys.platform != "darwin":
            raise ValueError("the qualified MLX route requires Apple Silicon macOS")
        for key in ["HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_HUB_DISABLE_TELEMETRY", "PYTHONNOUSERSITE"]:
            if os.environ.get(key) != "1":
                raise ValueError("missing isolated offline runtime environment")
        wire = request.to_wire()
        if (request.constraints.conditioning != "bridge" or request.constraints.motion != "still"
                or request.provider.pack_id != "ltx-2.3-q4-development"
                or request.provider.pack_version != "56a5866d"
                or request.provider.runtime_id != "ltx-mlx-development"
                or request.provider.runtime_version != "0.15.8+deadpan1"
                or request.provider.seed >= 2**32):
            raise ValueError("unsupported development provider or hold constraints")
        root = os.open(".", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            raw = contained_read(root, request.input.manifest, 256 * 1024)
            if hashlib.sha256(raw).hexdigest() != request.input.sha256:
                raise ValueError("context manifest hash mismatch")
            context = strict_json(raw)
            exact_keys(context, ["schema_version", "model_color", "plan", "left", "right", "input_color_interpretation"])
            if (type(context["schema_version"]) is not int or context["schema_version"] != 1
                    or context["model_color"] != "srgb"
                    or not isinstance(context["input_color_interpretation"], str)
                    or not 1 <= len(context["input_color_interpretation"]) <= 4096):
                raise ValueError("unsupported context or color interpretation")
            validate_bridge_context(context, request, wire["constraints"]["video"])
            inputs = []
            for name in ["left", "right"]:
                reference = context[name]
                exact_keys(reference, ["reference", "sha256", "byte_length"])
                data = contained_read(root, reference["reference"], 16 * 1024 * 1024)
                if (type(reference["byte_length"]) is not int or len(data) != reference["byte_length"]
                        or hashlib.sha256(data).hexdigest() != reference["sha256"]):
                    raise ValueError("conditioning input hash or length mismatch")
                inputs.append(data)
        finally:
            os.close(root)
        # This adapter supports only the host's new, fixed outputs directory.
        # Rust subsequently revalidates containment by descriptor after exit.
        if request.output_workspace != "outputs":
            raise ValueError("unsupported output workspace")
        output = Path.cwd() / "outputs"
        if output.is_symlink() or not output.is_dir() or any(output.iterdir()):
            raise ValueError("worker output directory must be new and empty")
        with args.runtime_config.open("rb") as stream:
            runtime_bytes = stream.read(64 * 1024 + 1)
        if len(runtime_bytes) > 64 * 1024:
            raise ValueError("runtime configuration exceeds its budget")
        from mlx_backend import generate, runtime_paths
        paths = runtime_paths(strict_json(runtime_bytes), check_cancel)
        native, provenance, _report = generate(
            paths, wire, context, inputs, output, stage, check_cancel, ADAPTER_SOURCES
        )
        check_cancel()
        native_artifact = finished_artifact(
            output, native, "outputs/native.mp4", 16 * 1024**3, check_cancel
        )
        provenance_artifact = finished_artifact(
            output, provenance, "outputs/provenance.json", 4 * 1024**2, check_cancel
        )
        native_plan = request.plan["native"]
        native_video = VideoSpec(
            native_plan["frame_count"],
            FrameRate(
                native_plan["frame_rate"]["numerator"],
                native_plan["frame_rate"]["denominator"],
            ),
            native_plan["width"],
            native_plan["height"],
        )
        finished.set()
        protocol.emit_completed_bridge(
            NativeCandidateManifest(
                native_artifact,
                provenance_artifact,
                native_video,
                request.provider,
            )
        )
    except Cancelled:
        finished.set()
        protocol.emit_cancelled()
    except Exception as error:
        finished.set()
        traceback.print_exc(file=sys.stderr)
        detail = (str(error).replace("\0", " ") or type(error).__name__).encode("utf-8")[:4000].decode("utf-8", "ignore")
        protocol.emit_failed("resource_exhausted" if isinstance(error, MemoryError) else "backend_failure", detail)


if __name__ == "__main__":
    run()
