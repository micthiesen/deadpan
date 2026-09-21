"""Pinned development adapter for the measured LTX-2.3 q4 keyframe route.

This is not a model manager or distributable runtime. All executable paths are
host-selected. The pack receipt names data files, never imported Python code.
"""

import hashlib
import io
import json
from pathlib import Path
import resource
import sys
import time

from worker_media import encode_rgb, exact_keys, file_digest, sampled_rgb, verify_rgb
from runtime_source import loaded_sources, verify_roots, verify_tree


RUNTIME_COMMIT = "3392d75934120b7e69eefbe55893f7ef82be92a4"
PACK_REVISION = "56a5866d638ecfe37c54d348e88938235185c2d4"
GEMMA_REVISION = "86cc6a8dedbc456dd0e4af01a9d09f396f77e558"
PROMPT_VERSION = "deadpan-hold-1"
PROMPT = ("Locked camera. The person maintains the same identity, pose, expression, "
          "and composition, with quiet minimal natural motion. No speech, no new "
          "objects, no scene change.")
EVIDENCE = Path(__file__).resolve().parent / "evidence/2026-09-20-smoke"


def runtime_paths(config, check_cancel):
    exact_keys(config, ["runtime_source", "model_cache", "ffmpeg", "ffprobe"])
    paths = {key: Path(value) for key, value in config.items()}
    if any(not path.is_absolute() for path in paths.values()):
        raise ValueError("runtime paths must be absolute host-selected paths")
    source_manifest = json.loads((Path(__file__).parent / "ltx-source-manifest.json").read_text())
    if source_manifest["commit"] != RUNTIME_COMMIT:
        raise ValueError("unexpected source manifest revision")
    verify_tree(paths["runtime_source"], source_manifest, check_cancel)
    verify_roots(paths["runtime_source"])
    paths["source_manifest"] = source_manifest
    receipt = json.loads((EVIDENCE / "download-manifest.json").read_text())
    roots = {
        "dgrauet/ltx-2.3-mlx-q4": paths["model_cache"] / "mlx_ltx_q4_pack" / PACK_REVISION,
        "mlx-community/gemma-3-12b-it-4bit": paths["model_cache"] / "mlx_gemma_default_text_encoder" / GEMMA_REVISION,
    }
    verified_assets = []
    for asset in receipt["assets"]:
        check_cancel()
        path = roots[asset["repository"]] / asset["path"]
        digest, length = hashlib.sha256(), 0
        with path.open("rb") as stream:
            while data := stream.read(1024 * 1024):
                check_cancel()
                length += len(data)
                if length > asset["size"]:
                    raise ValueError(f"model data grew beyond its receipt: {asset['path']}")
                digest.update(data)
        if length != asset["size"] or digest.hexdigest() != asset["sha256"]:
            raise ValueError(f"model data mismatch: {asset['path']}")
        verified_assets.append({
            "repository": asset["repository"],
            "path": asset["path"],
            "size": asset["size"],
            "sha256": asset["sha256"],
        })
    paths["model"] = roots["dgrauet/ltx-2.3-mlx-q4"]
    paths["gemma"] = roots["mlx-community/gemma-3-12b-it-4bit"]
    paths["verified_assets"] = verified_assets
    return paths


def generate(paths, request, context, input_bytes, output, stage, check_cancel, adapter_sources):
    started = time.monotonic()
    stage("runtime_loading")
    import mlx.core as mx
    import mlx_lm
    import numpy as np
    from PIL import Image
    from ltx_core_mlx.components.guiders import MultiModalGuiderParams
    from ltx_core_mlx.model.video_vae.video_vae import _compute_decode_tiling, decode_cache_limit
    from ltx_core_mlx.text_encoders.gemma.encoders.base_encoder import GemmaLanguageModel
    from ltx_pipelines_mlx.keyframe_interpolation import KeyframeInterpolationPipeline
    from ltx_pipelines_mlx.utils import media_io

    loaded_sources(paths["runtime_source"], paths["source_manifest"])

    # Conditioning's native CRF-33 round trip also invokes FFmpeg. Never search
    # the user's PATH, and do not silently bypass that model preprocessing.
    media_io.find_ffmpeg = lambda: str(paths["ffmpeg"])

    # A pinned source checkout is part of this host-controlled developer runtime.
    import ltx_pipelines_mlx.keyframe_interpolation as imported_pipeline
    expected_module = paths["runtime_source"] / "packages/ltx-pipelines-mlx/src/ltx_pipelines_mlx/keyframe_interpolation.py"
    if Path(imported_pipeline.__file__).resolve() != expected_module.resolve():
        raise ValueError("Python imported an unexpected pipeline installation")

    def load_local_gemma(self, model_path=None):
        check_cancel()
        path = model_path or self._model_path
        if Path(path).resolve() != paths["gemma"].resolve():
            raise ValueError("unexpected text encoder location")
        self._model, self._tokenizer = mlx_lm.load(
            path, tokenizer_config={"trust_remote_code": False, "local_files_only": True})

    GemmaLanguageModel.load = load_local_gemma
    plan = context["plan"]
    count, native_count = plan["project"]["interior_frames"], plan["native"]["frame_count"]
    width, height = plan["native"]["width"], plan["native"]["height"]
    images = []
    for data in input_bytes:
        with Image.open(io.BytesIO(data)) as image:
            if image.format != "PNG" or image.size != (width, height) or image.mode != "RGB":
                raise ValueError("conditioning must be prepared RGB PNGs at the exact model grid")
            # Decode from the same hash-verified bytes, not a reopened worker path.
            image.load()
            images.append(image.copy())
    check_cancel()
    mx.set_memory_limit(64 * 1024**3)  # MLX guideline, not a hard memory limit.
    mx.set_cache_limit(8 * 1024**3)
    mx.reset_peak_memory()
    report = {"schema_version": 2, "runtime_commit": RUNTIME_COMMIT,
              "adapter_sources_sha256": adapter_sources,
              "pack_revision": PACK_REVISION,
              "request_binding": {key: request[key] for key in
                                  ["identity", "project_id", "revision_id", "target", "input",
                                   "constraints", "provider", "plan"]},
              "verified_assets": paths["verified_assets"],
              "gemma_revision": GEMMA_REVISION, "prompt_version": PROMPT_VERSION,
              "prompt": PROMPT, "seed": request["provider"]["seed"], "context": context,
              "model_color_interpretation": "full-range SDR sRGB RGB, BT.709 primaries",
              "temporal_interpolation": "linear in encoded sRGB, half-up RGB8 quantization",
              "conditioning_preprocessing": "pinned upstream CRF-33 H.264 round trip, resize and center crop at both stage resolutions; original prepared PNGs retained",
              "configuration": {"stage1_steps": 20, "stage2_steps": 3, "cfg_scale": 3.0,
                                "low_memory": True, "low_ram_streaming": False,
                                "generate_audio": False},
              "device_info": mx.device_info()}

    class BridgePipeline(KeyframeInterpolationPipeline):
        def _stepwise_hook(self, latent_frames, latent_height, latent_width, *, stage=None):
            del latent_frames, latent_height, latent_width
            check_cancel()
            emit_stage("inference")

            def on_step(index, total, video_x0, sigma):
                del video_x0, sigma
                check_cancel()
                print(f"denoise stage={stage} completed={index + 1} total={total}", file=sys.stderr, flush=True)
            return on_step

        def _decode_and_save_video(self, video_latent, audio_latent, output_path, *, frame_rate, seed=0):
            del audio_latent, output_path, seed
            check_cancel()
            emit_stage("decoding")
            decoder = self.video_decoder_block.load()
            tiling = _compute_decode_tiling(video_latent.shape, frame_rate=frame_rate)
            native_frames = []
            with decode_cache_limit():
                for chunk in decoder.tiled_decode(video_latent, tiling):
                    if tuple(chunk.shape[:2]) != (1, 3) or tuple(chunk.shape[3:]) != (height, width):
                        raise ValueError("unexpected VAE RGB geometry")
                    for index in range(chunk.shape[2]):
                        check_cancel()
                        if len(native_frames) >= native_count:
                            raise ValueError("VAE emitted too many native frames")
                        # Match pinned upstream RGB8 quantization before interpolation.
                        frame = ((mx.clip(chunk[0, :, index], -1, 1) + 1) * 127.5).astype(mx.uint8)
                        frame = mx.contiguous(frame.transpose(1, 2, 0))
                        mx.eval(frame)
                        native_frames.append(np.array(frame, copy=True))
            if len(native_frames) != native_count:
                raise ValueError("VAE emitted an unexpected native duration")
            check_cancel()
            emit_stage("encoding")
            native_path, candidate_path = output / "native.mp4", output / "candidate.mp4"
            report["native_rgb"] = encode_rgb(
                paths["ffmpeg"], native_path, (frame.tobytes() for frame in native_frames),
                width, height, plan["native"]["frame_rate"], check_cancel)
            report["candidate_rgb"] = encode_rgb(
                paths["ffmpeg"], candidate_path, sampled_rgb(native_frames, count, check_cancel),
                width, height, plan["project"]["frame_rate"], check_cancel)
            emit_stage("worker_validation")
            for name, path, frame_rate in [
                ("native", native_path, plan["native"]["frame_rate"]),
                ("candidate", candidate_path, plan["project"]["frame_rate"]),
            ]:
                report[f"{name}_probe"] = verify_rgb(
                    paths["ffmpeg"], paths["ffprobe"], path, report[f"{name}_rgb"],
                    width, height, frame_rate, check_cancel)
                report[f"{name}_sha256"] = file_digest(path)
                report[f"{name}_bytes"] = path.stat().st_size
            return str(candidate_path)

    emit_stage = stage
    stage("model_loading")
    pipe = BridgePipeline(
        model_dir=str(paths["model"]), gemma_model_id=str(paths["gemma"]),
        low_memory=True, low_ram_streaming=False, dev_transformer="transformer-dev.safetensors",
        distilled_lora="ltx-2.3-22b-distilled-lora-384.safetensors",
    )
    pipe.generate_audio = False
    pipe.verbose = True
    check_cancel()
    pipe.generate_and_save(
        prompt=PROMPT, output_path=str(output / "candidate.mp4"), keyframe_images=images,
        keyframe_indices=[0, native_count - 1], keyframe_strengths=[1.0, 1.0],
        height=height, width=width, num_frames=native_count, frame_rate=24, seed=request["provider"]["seed"],
        stage1_steps=20, stage2_steps=3, cfg_scale=3.0,
        video_guider_params=MultiModalGuiderParams(cfg_scale=3.0, stg_scale=1.0, rescale_scale=0.7,
                                                 modality_scale=3.0, stg_blocks=[28]),
        audio_guider_params=MultiModalGuiderParams(cfg_scale=7.0, stg_scale=1.0, rescale_scale=0.7,
                                                 modality_scale=3.0, stg_blocks=[28]),
    )
    check_cancel()
    mx.synchronize()
    report["loaded_ltx_sources_sha256"] = loaded_sources(paths["runtime_source"], paths["source_manifest"])
    report.update(backend_seconds=time.monotonic() - started,
                  process_peak_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                  mlx_counter_at_end_bytes=mx.get_peak_memory(),
                  status="worker_validated_not_host_validated_or_accepted")
    provenance_path = output / "provenance.json"
    with provenance_path.open("x") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    return output / "native.mp4", provenance_path, report
