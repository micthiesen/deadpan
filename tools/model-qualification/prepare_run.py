"""Prepare an isolated developer probe from already-installed runtime and data.

Run with the pinned private environment (Pillow is already an MLX dependency).
No downloads or environment installation occur here.
"""

import argparse
from fractions import Fraction
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import uuid


def save(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-source", required=True, type=Path)
    parser.add_argument("--model-cache", required=True, type=Path)
    parser.add_argument("--ffmpeg", required=True, type=Path)
    parser.add_argument("--ffprobe", required=True, type=Path)
    parser.add_argument("--left", required=True, type=Path)
    parser.add_argument("--right", required=True, type=Path)
    parser.add_argument("--input-color-interpretation", required=True)
    parser.add_argument("--run-directory", required=True, type=Path)
    parser.add_argument("--frames", type=int, default=24)
    parser.add_argument("--fps", default="24/1")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--cancel-after-millis", type=int)
    parser.add_argument("--cancel-at-stage", choices=["preflight", "runtime_loading", "model_loading", "inference", "decoding", "encoding", "worker_validation"])
    args = parser.parse_args()
    if not 1 <= args.frames <= 180 or not 0 <= args.seed < 2**32:
        parser.error("frame count or seed outside this probe envelope")
    root = Path(__file__).resolve().parents[2]
    run = args.run_directory.absolute()
    run.mkdir()
    workspace = run / "worker"
    inputs = workspace / "inputs"
    inputs.mkdir(parents=True)
    (workspace / "outputs").mkdir()
    fps = Fraction(args.fps)
    plan = json.loads(subprocess.check_output([
        str(root / "target/debug/examples/plan_mlx_bridge"), str(args.frames),
        str(fps.numerator), str(fps.denominator),
    ]))
    from PIL import Image, ImageOps
    references = []
    for name, path in [("left", args.left), ("right", args.right)]:
        original = path.read_bytes()
        with Image.open(io.BytesIO(original)) as image:
            image.load()
            resized = ImageOps.contain(image.convert("RGB"), (768, 320), method=Image.Resampling.LANCZOS)
            prepared = Image.new("RGB", (768, 320))
            offset = ((768 - resized.width) // 2, (320 - resized.height) // 2)
            prepared.paste(resized, offset)
            destination = inputs / f"{name}.png"
            prepared.save(destination)
            data = destination.read_bytes()
            references.append({"reference": f"inputs/{name}.png", "sha256": hashlib.sha256(data).hexdigest(),
                               "byte_length": len(data)})
            save(inputs / f"{name}-preparation.json", {
                "original_sha256": hashlib.sha256(original).hexdigest(), "original_size": list(image.size),
                "resized_size": list(resized.size), "offset": list(offset),
                "padding_rgb": [0, 0, 0], "resampling": "Lanczos",
                "input_color_interpretation": args.input_color_interpretation,
                "model_color_interpretation": "full-range SDR sRGB RGB, BT.709 primaries",
                "color_transform_performed": False,
            })
    context = {"schema_version": 1, "model_color": "srgb", "plan": plan,
               "left": references[0], "right": references[1],
               "input_color_interpretation": args.input_color_interpretation}
    save(inputs / "context.json", context)
    save(run / "runtime.json", {key: str(getattr(args, key).absolute()) for key in
                               ["runtime_source", "model_cache", "ffmpeg", "ffprobe"]})
    identity = uuid.uuid4().hex
    request = {
        "operation": "generate_bridge", "protocol": 2,
        "identity": {"request_id": f"probe-{identity}", "attempt_id": "attempt-1"},
        "cancellation_token": uuid.uuid4().hex, "project_id": f"probe-{identity}", "revision_id": "revision-1",
        "target": {"hold_id": "hold-1", "request_version": 1},
        "input": {"manifest": "inputs/context.json", "sha256": hashlib.sha256((inputs / "context.json").read_bytes()).hexdigest()},
        "output_workspace": "outputs",
        "constraints": {"conditioning": "bridge", "motion": "still", "video": {
            "frames": args.frames, "frame_rate": {"numerator": fps.numerator, "denominator": fps.denominator},
            "width": 768, "height": 320}},
        "provider": {"pack_id": "ltx-2.3-q4-development", "pack_version": "56a5866d",
                     "runtime_id": "ltx-mlx-development", "runtime_version": "0.15.8+deadpan1", "seed": args.seed},
        "plan": plan,
    }
    retention = {
        "workspace": str(workspace), "request": request, "input_scope": "inputs",
        "manifest": {"reference": "inputs/context.json", "sha256": request["input"]["sha256"],
                     "byte_length": (inputs / "context.json").stat().st_size},
        "limits": {"maximum_manifest_bytes": 1024 * 1024, "maximum_frame_bytes": 64 * 1024 * 1024,
                   "timeout_ms": 30_000},
        "output_directory": str(run / "retained-conditioning"),
    }
    save(run / "retention-config.json", retention)
    subprocess.run([
        str(root / "target/debug/examples/retain_bridge_conditioning"),
        str(run / "retention-config.json"),
    ], check=True, stdout=subprocess.DEVNULL)
    save(run / "host-config.json", {
        "executable": sys.executable, "worker_script": str(root / "tools/model-qualification/worker.py"),
        "runtime_config": str(run / "runtime.json"), "workspace": str(workspace),
        "report_directory": str(run / "host"), "request": request,
        "cancel_after_millis": args.cancel_after_millis,
        "cancel_at_stage": args.cancel_at_stage,
    })
    print(run / "host-config.json")


if __name__ == "__main__":
    main()
