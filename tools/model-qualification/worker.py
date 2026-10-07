"""One framed, offline MLX qualification attempt. Run only via the Rust host.

The host owns runtime selection, process termination and output authority. This
developer adapter produces an unaccepted candidate, never a project mutation.
"""

import argparse
import hashlib
from fractions import Fraction
import json
import os
from pathlib import Path
import resource
import re
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

from worker_media import exact_keys, integer, validate_plan
from worker_protocol import (
    FrameRate,
    GenerateBridgeRequest,
    NativeCandidateManifest,
    VideoSpec,
    WorkerProtocol,
    WorkspaceArtifact,
    _core_identifier,
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


# The only colour space this model takes and produces: full-range sRGB RGB
# with BT.709 primaries (deadpan_models::CANONICAL_BRIDGE_COLOR).
MODEL_COLOR_SPACE = {"transfer": "srgb", "primaries": "bt709", "matrix": "rgb", "range": "full"}
RUST_WHITESPACE = "\t\n\v\f\r \u0085\u00a0\u1680" + "".join(
    chr(codepoint) for codepoint in range(0x2000, 0x200B)
) + "\u2028\u2029\u202f\u205f\u3000"


def valid_input_color_interpretation(value):
    """Match Rust's trimmed nonempty, NUL-free, 4096 UTF-8-byte string rule."""
    if not isinstance(value, str) or "\0" in value:
        return False
    try:
        if len(value.encode("utf-8")) > 4096:
            return False
    except UnicodeEncodeError:
        return False
    return any(character not in RUST_WHITESPACE for character in value)


def validate_context_shape(context):
    """Admit explicit definition clocks and older retained context grammars.

    The host records and checks boundary evidence; this worker validates the
    geometry binding before loading the model and refuses an unsupported model
    colour space.
    """
    if not isinstance(context, dict) or type(context.get("schema_version")) is not int:
        raise ValueError("unsupported context or color interpretation")
    if context["schema_version"] in (2, 3, 4, 5):
        keys = ["schema_version", "model_color_space", "plan", "left", "right",
                "input_color_interpretation", "boundaries"]
        if context["schema_version"] >= 3:
            keys.append("geometry")
        if context["schema_version"] >= 4:
            keys.append("region")
        exact_keys(context, keys)
        boundaries = context["boundaries"]
        if not isinstance(boundaries, dict):
            raise ValueError("unsupported context boundaries")
        exact_keys(boundaries, ["left", "right"])
        supported = context["model_color_space"] == MODEL_COLOR_SPACE
        if context["schema_version"] >= 3:
            plan = context.get("plan")
            native = plan.get("native") if isinstance(plan, dict) else None
            if not isinstance(native, dict):
                raise ValueError("unsupported context geometry raster")
            validate_context_geometry(
                context,
                integer(native.get("width"), 1, (1 << 32) - 1),
                integer(native.get("height"), 1, (1 << 32) - 1),
            )
        if context["schema_version"] >= 4:
            validate_context_region(context)
        if context["schema_version"] == 5:
            validate_definition_clocks(context)
    elif context["schema_version"] == 1:
        exact_keys(context, ["schema_version", "model_color", "plan", "left", "right",
                             "input_color_interpretation"])
        supported = context["model_color"] == "srgb"
    else:
        supported = False
    if (not supported
            or not valid_input_color_interpretation(context["input_color_interpretation"])):
        raise ValueError("unsupported context or color interpretation")


def validate_definition_clocks(context):
    """Require one immutable authored definition and an exact N+1 span."""
    clocks = []
    positions = []
    for side in ("left", "right"):
        payload = next(iter(context["boundaries"][side].values()))
        clock = payload["clock"]
        exact_keys(clock, ["kind", "project_id", "revision_id", "definition", "position"])
        if clock["kind"] != "definition":
            raise ValueError("schema5 requires definition boundary clocks")
        for field in ("project_id", "revision_id", "definition"):
            _core_identifier(clock[field], field)
        exact_keys(clock["position"], ["numerator", "denominator"])
        values = []
        for field in ("numerator", "denominator"):
            text = clock["position"][field]
            if (not isinstance(text, str) or len(text) > 40
                    or re.fullmatch(r"[+-]?[0-9]+", text) is None):
                raise ValueError("invalid definition position")
            values.append(integer(int(text), -(1 << 127), (1 << 127) - 1))
        numerator, denominator = values
        if numerator < 0 or denominator <= 0:
            raise ValueError("invalid definition position")
        positions.append(Fraction(numerator, denominator))
        clocks.append(tuple(clock[field] for field in ("project_id", "revision_id", "definition")))
    project = context["plan"].get("project")
    if not isinstance(project, dict):
        raise ValueError("missing boundary plan")
    frames = integer(project.get("interior_frames"), 1, (1 << 63) - 2)
    if clocks[0] != clocks[1] or positions[1] - positions[0] != frames + 1:
        raise ValueError("boundary pictures do not enclose the planned Hold in one clock")


def validate_context_geometry(context, native_width, native_height):
    """Validate schema-3 rects against the native raster and boundary types."""
    geometry = context["geometry"]
    exact_keys(geometry, ["presentation", "left_content", "right_content"])
    native_width = integer(native_width, 1, (1 << 32) - 1)
    native_height = integer(native_height, 1, (1 << 32) - 1)

    def rect(value, label, *, contained_by=None):
        if not isinstance(value, dict):
            raise ValueError(f"unsupported {label} rectangle")
        exact_keys(value, ["x", "y", "width", "height"])
        x = integer(value["x"], 0, (1 << 32) - 1)
        y = integer(value["y"], 0, (1 << 32) - 1)
        width = integer(value["width"], 1, (1 << 32) - 1)
        height = integer(value["height"], 1, (1 << 32) - 1)
        right, bottom = x + width, y + height
        if right > (1 << 32) - 1 or bottom > (1 << 32) - 1:
            raise ValueError(f"{label} rectangle overflows u32")
        if (width > native_width or height > native_height
                or right > native_width or bottom > native_height):
            raise ValueError(f"{label} rectangle exceeds the native raster")
        if x != (native_width - width) // 2 or y != (native_height - height) // 2:
            raise ValueError(f"{label} rectangle is not centered in the native raster")
        if contained_by is not None:
            parent_right = contained_by["x"] + contained_by["width"]
            parent_bottom = contained_by["y"] + contained_by["height"]
            if (x < contained_by["x"] or y < contained_by["y"]
                    or right > parent_right or bottom > parent_bottom):
                raise ValueError(f"{label} rectangle is outside the presentation")
        return {"x": x, "y": y, "width": width, "height": height}

    presentation = rect(geometry["presentation"], "presentation")
    boundaries = context["boundaries"]
    for side, boundary_key in (("left_content", "left"), ("right_content", "right")):
        boundary = boundaries[boundary_key]
        if not isinstance(boundary, dict) or len(boundary) != 1:
            raise ValueError("unsupported boundary picture")
        kind, payload = next(iter(boundary.items()))
        if kind not in {"original", "generated", "authored_black"} or not isinstance(payload, dict):
            raise ValueError("unsupported boundary picture")
        clock_field = "clock" if context["schema_version"] == 5 else "project_frame"
        fields = {
            "original": [clock_field, "asset", "qualification", "picture"],
            "generated": [clock_field, "sampled_asset", "sampled_object", "provenance", "picture"],
            "authored_black": [clock_field],
        }[kind]
        exact_keys(payload, fields)
        if clock_field == "project_frame":
            integer(payload["project_frame"], 0, (1 << 63) - 1)
        is_black = kind == "authored_black"
        content = geometry[side]
        if (content is None) != is_black:
            raise ValueError("content geometry must be absent only for authored black")
        if content is not None:
            rect(content, side, contained_by=presentation)


def validate_context_region(context):
    """Check the captured subject's exact Original coordinates and coverage."""
    region = context["region"]
    if not isinstance(region, dict):
        raise ValueError("unsupported captured region")
    if region.get("selection") == "none":
        exact_keys(region, ["selection"])
        return
    if region.get("selection") != "selected":
        raise ValueError("unsupported region selection")
    exact_keys(region, ["selection", "target", "label", "target_sha256", "left", "right"])
    _core_identifier(region["target"], "region target")
    label = region["label"]
    digest = region["target_sha256"]
    if (not isinstance(label, str) or not label.strip(RUST_WHITESPACE)
            or len(label.encode("utf-8")) > 128 or not isinstance(digest, str)
            or len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest)):
        raise ValueError("invalid captured region identity")
    reasons = {"not_original", "different_asset", "outside_target_span", "lost_track",
               "interpolated_track", "low_confidence", "outside_source", "missing_content", "too_small"}
    for side in ("left", "right"):
        seed = region[side]
        if not isinstance(seed, dict):
            raise ValueError("unsupported captured region boundary")
        if seed.get("status") == "unavailable":
            exact_keys(seed, ["status", "reason"])
            if seed["reason"] not in reasons:
                raise ValueError("unsupported region unavailable reason")
            continue
        if seed.get("status") != "available":
            raise ValueError("unsupported region boundary status")
        exact_keys(seed, ["status", "asset", "point", "source", "region", "confidence"])
        boundary = context["boundaries"][side].get("original")
        if boundary is None or seed["asset"] != boundary["asset"]:
            raise ValueError("region seed requires the same Original boundary")
        exact_keys(seed["point"], ["ticks", "time_base"])
        pts = boundary["picture"]["pts"]
        if (seed["point"]["time_base"] != pts["time_base"]
                or seed["point"]["ticks"] != {"numerator": str(pts["ticks"]), "denominator": "1"}):
            raise ValueError("region seed differs from the decoded Original PTS")
        if seed["source"] in ("initial", "manual"):
            if seed["confidence"] is not None:
                raise ValueError("authored region has detector confidence")
        elif seed["source"] == {"tracked": "tracked"}:
            integer(seed["confidence"], 700, 1000)
        else:
            raise ValueError("region seed has unavailable authored tracking evidence")
        rectangle = seed["region"]
        exact_keys(rectangle, ["center", "size"])
        if any(not isinstance(rectangle[key], list) or len(rectangle[key]) != 2
               for key in ("center", "size")):
            raise ValueError("invalid captured region dimensions")
        for center, size in zip(rectangle["center"], rectangle["size"]):
            center = integer(center, 0, 1_000_000)
            size = integer(size, 1, 1_000_000)
            if 2 * center < size or 2 * center + size > 2_000_000:
                raise ValueError("region seed extends outside its Original")
        if context["geometry"][side + "_content"] is None:
            raise ValueError("region seed has no fitted content")
        content = context["geometry"][side + "_content"]
        native = context["plan"]["native"]
        for size, axis in zip(rectangle["size"], ("width", "height")):
            if size * content[axis] * 4096 < 1_000_000 * native[axis]:
                raise ValueError("region seed is too small in the conditioning picture")


def validate_bridge_context(context, request, video):
    if not isinstance(request, GenerateBridgeRequest):
        raise ValueError("development worker requires protocol-2 generate_bridge")
    if not isinstance(context, dict) or context.get("plan") != request.plan:
        raise ValueError("request plan differs from context plan")
    if "schema_version" in context:
        validate_context_shape(context)
    native_width = request.plan["native"]["width"]
    native_height = request.plan["native"]["height"]
    result = validate_plan(context["plan"], video)
    if context.get("schema_version", 0) >= 3:
        validate_context_geometry(context, native_width, native_height)
    if context.get("schema_version") == 5:
        for side in ("left", "right"):
            clock = next(iter(context["boundaries"][side].values()))["clock"]
            if clock["project_id"] != request.project_id or clock["revision_id"] != request.revision_id:
                raise ValueError("definition boundary differs from the worker origin")
    capture = context.get("region", {"selection": "none"})
    captured_target = capture.get("target") if capture["selection"] == "selected" else None
    if captured_target != request.constraints.region_target:
        raise ValueError("region target differs from captured context")
    return result


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


def check(runtime_config):
    """`--check`: a model pack smoke test without the framed protocol."""
    for key in ["HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_HUB_DISABLE_TELEMETRY", "PYTHONNOUSERSITE"]:
        os.environ[key] = "1"
    with runtime_config.open("rb") as stream:
        runtime_bytes = stream.read(64 * 1024 + 1)
    if len(runtime_bytes) > 64 * 1024:
        raise ValueError("runtime configuration exceeds its budget")
    from mlx_backend import check_runtime
    report = check_runtime(strict_json(runtime_bytes))
    report["adapter_sources_sha256"] = ADAPTER_SOURCES
    sys.stdout.write(json.dumps(report) + "\n")


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-config", required=True, type=Path)
    parser.add_argument("--check", action="store_true",
                        help="verify the runtime and model data, then exit; no protocol")
    args = parser.parse_args()
    if args.check:
        check(args.runtime_config)
        return
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
        if request.constraints.conditioning != "bridge" or request.provider.seed >= 2**32:
            raise ValueError("unsupported hold constraints or seed")
        root = os.open(".", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            raw = contained_read(root, request.input.manifest, 256 * 1024)
            if hashlib.sha256(raw).hexdigest() != request.input.sha256:
                raise ValueError("context manifest hash mismatch")
            context = strict_json(raw)
            validate_context_shape(context)
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
        paths = runtime_paths(strict_json(runtime_bytes), check_cancel, provider=request.provider)
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
