"""Exact, bounded RGB preparation for the development-only MLX adapter.

The adapter explicitly interprets model RGB as full-range SDR sRGB. These
helpers do not establish a source color transform or a shipping codec choice.
"""

from fractions import Fraction
import hashlib
import json
from pathlib import Path
import subprocess


COLOR = {"color_range": "pc", "color_space": "gbr",
         "color_transfer": "iec61966-2-1", "color_primaries": "bt709"}


def exact_keys(value, keys):
    if not isinstance(value, dict) or set(value) != set(keys):
        raise ValueError(f"expected fields {sorted(keys)}")


def integer(value, low, high):
    if type(value) is not int or not low <= value <= high:
        raise ValueError("integer outside the qualification envelope")
    return value


def rate(value):
    exact_keys(value, ["numerator", "denominator"])
    return Fraction(integer(value["numerator"], 1, 120_000),
                    integer(value["denominator"], 1, 100_000))


def ratio(value):
    exact_keys(value, ["numerator", "denominator"])
    for key in value:
        if not isinstance(value[key], str) or len(value[key]) > 80:
            raise ValueError("invalid exact ratio")
    numerator, denominator = int(value["numerator"]), int(value["denominator"])
    if denominator <= 0:
        raise ValueError("exact ratio denominator must be positive")
    return Fraction(numerator, denominator)


def validate_plan(plan, video):
    """Recheck the host plan against this adapter's deliberately narrow envelope."""
    exact_keys(plan, ["schema_version", "operation", "interpolation", "project",
                      "native", "timing", "sampling"])
    if type(plan["schema_version"]) is not int or plan["schema_version"] != 1:
        raise ValueError("unsupported plan schema")
    if plan["operation"] != "bridge" or plan["interpolation"] != "linear":
        raise ValueError("only an explicit linear bridge is supported")
    if plan["sampling"] != {"endpoint_policy": "interior_only"}:
        raise ValueError("conditioning endpoints cannot become inserted frames")
    project, native, timing = plan["project"], plan["native"], plan["timing"]
    exact_keys(project, ["interior_frames", "frame_rate"])
    exact_keys(native, ["frame_count", "frame_rate", "width", "height"])
    exact_keys(timing, ["requested_boundary_duration", "actual_boundary_duration",
                        "retime_deviation"])
    count = integer(project["interior_frames"], 1, 180)
    model_count = integer(native["frame_count"], 9, 97)
    project_rate, native_rate = rate(project["frame_rate"]), rate(native["frame_rate"])
    if not Fraction(1) <= project_rate <= 120 or native_rate != 24:
        raise ValueError("unsupported frame rate")
    if (native["width"], native["height"]) != (768, 320) or model_count % 8 != 1:
        raise ValueError("unsupported model dimensions or frame count")
    exact_keys(video, ["frames", "frame_rate", "width", "height"])
    integer(video["frames"], 1, 180)
    integer(video["width"], 1, 32768)
    integer(video["height"], 1, 32768)
    rate(video["frame_rate"])
    if video != {"frames": count, "frame_rate": project["frame_rate"],
                 "width": native["width"], "height": native["height"]}:
        raise ValueError("plan does not match the authored request")
    requested, actual = Fraction(count + 1) / project_rate, Fraction(model_count - 1) / native_rate
    if (ratio(timing["requested_boundary_duration"]) != requested
            or ratio(timing["actual_boundary_duration"]) != actual
            or ratio(timing["retime_deviation"]) != actual - requested):
        raise ValueError("plan timing does not match its counts and rates")
    # Reject requests outside available duration, rather than loading a model
    # for a silently clamped result. Legal rounding inside this range is explicit.
    ideal = requested * native_rate + 1
    if not 9 <= ideal <= 97:
        raise ValueError("requested boundary duration is outside this adapter")
    chosen = min(range(9, 98, 8), key=lambda candidate: (abs(candidate - ideal), -candidate))
    if model_count != chosen:
        raise ValueError("native length is not the nearest legal duration")
    return count, model_count


def sample_positions(count, model_count):
    integer(count, 1, 180)
    integer(model_count, 2, 97)
    for index in range(count):
        lower, remainder = divmod((index + 1) * (model_count - 1), count + 1)
        yield lower, lower + (remainder != 0), remainder, count + 1


def blend_channel(left, right, numerator, denominator):
    """Half-up quantization after exact linear interpolation in encoded sRGB."""
    return (left * (denominator - numerator) + right * numerator + denominator // 2) // denominator


def sampled_rgb(frames, count, check_cancel):
    # MLX already depends on NumPy. It stays out of protocol/preflight imports.
    import numpy as np
    for lower, upper, numerator, denominator in sample_positions(count, len(frames)):
        check_cancel()
        if numerator == 0:
            yield frames[lower].tobytes()
        else:
            values = blend_channel(frames[lower].astype(np.uint32),
                                   frames[upper].astype(np.uint32), numerator, denominator)
            yield values.astype(np.uint8).tobytes()


def encode_rgb(ffmpeg, destination, frames, width, height, frame_rate, check_cancel):
    """Lossless developer RGB intermediate; NOT the selected app/export encoder."""
    fps = rate(frame_rate)
    command = [str(ffmpeg), "-hide_banner", "-loglevel", "error", "-nostdin", "-n",
               "-f", "rawvideo", "-pix_fmt", "rgb24", "-video_size", f"{width}x{height}",
               "-framerate", f"{fps.numerator}/{fps.denominator}", "-i", "pipe:0", "-an",
               "-vf", "setparams=range=full:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=gbr",
               "-c:v", "libx264rgb", "-crf", "0", "-preset", "ultrafast", "-pix_fmt", "rgb24",
               "-color_range", "pc", "-colorspace", "rgb", "-color_trc", "iec61966-2-1",
               "-color_primaries", "bt709", "-movflags", "+write_colr",
               "-video_track_timescale", str(fps.numerator), str(destination)]
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL)
    digest, count = hashlib.sha256(), 0
    try:
        for frame in frames:
            check_cancel()
            if len(frame) != width * height * 3 or count >= 180:
                raise ValueError("unexpected RGB frame size/count")
            process.stdin.write(frame)
            digest.update(frame)
            count += 1
        process.stdin.close()
        if process.wait(timeout=60) != 0 or count == 0:
            raise ValueError("RGB encode failed")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        if not process.stdin.closed:
            process.stdin.close()
    return {"frames": count, "rgb_sha256": digest.hexdigest(), "command": command}


def verify_rgb(ffmpeg, ffprobe, path, expected, width, height, frame_rate, check_cancel):
    """Decode the complete frozen file and compare its RGB bytes and timing."""
    fps = rate(frame_rate)
    probe = subprocess.run(
        [str(ffprobe), "-v", "error", "-show_streams", "-show_frames",
         "-show_entries", "frame=best_effort_timestamp,duration", "-of", "json", str(path)],
        check=True, capture_output=True, timeout=60,
    )
    if len(probe.stdout) > 64 * 1024:
        raise ValueError("media probe exceeded its budget")
    metadata = json.loads(probe.stdout)
    streams = metadata.get("streams", [])
    if len(streams) != 1 or streams[0].get("codec_type") != "video":
        raise ValueError("candidate must contain exactly one video stream")
    stream = streams[0]
    if (stream.get("width"), stream.get("height")) != (width, height):
        raise ValueError("encoded dimensions mismatch")
    if any(stream.get(key) != value for key, value in COLOR.items()):
        raise ValueError("encoded model color tags mismatch")
    if (Fraction(stream["r_frame_rate"]) != fps or Fraction(stream["avg_frame_rate"]) != fps
            or int(stream["start_pts"]) != 0
            or int(stream["duration_ts"]) * Fraction(stream["time_base"]) != expected["frames"] / fps):
        raise ValueError("encoded frame timing mismatch")
    frames = metadata.get("frames", [])
    if len(frames) != expected["frames"]:
        raise ValueError("probed frame count mismatch")
    clock = Fraction(stream["time_base"])
    for index, frame in enumerate(frames):
        if (int(frame["best_effort_timestamp"]) * clock != index / fps
                or int(frame["duration"]) * clock != 1 / fps):
            raise ValueError("encoded presentation timestamp/duration mismatch")
    process = subprocess.Popen(
        [str(ffmpeg), "-v", "error", "-xerror", "-nostdin", "-i", str(path),
         "-map", "0:v:0", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
        stdout=subprocess.PIPE,
    )
    digest, length = hashlib.sha256(), 0
    limit = expected["frames"] * width * height * 3
    try:
        while data := process.stdout.read(64 * 1024):
            check_cancel()
            length += len(data)
            if length > limit:
                raise ValueError("decoder emitted extra frames")
            digest.update(data)
        if process.wait(timeout=60) != 0 or length != limit or digest.hexdigest() != expected["rgb_sha256"]:
            raise ValueError("decoded frames do not match encoded RGB")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        process.stdout.close()
    return metadata


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()
