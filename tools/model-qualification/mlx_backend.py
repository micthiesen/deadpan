"""Pinned development adapter for the measured LTX-2.3 q4 keyframe route.

This is not a model manager or distributable runtime. All executable paths are
host-selected. The pack receipt names data files, never imported Python code.
"""

import hashlib
import io
import json
import os
from pathlib import Path
import resource
import re
import stat
import sys
import time

from worker_media import encode_rgb, exact_keys, file_digest, sampled_rgb, verify_rgb
from runtime_source import loaded_sources, verify_roots, verify_tree


RUNTIME_COMMIT = "3392d75934120b7e69eefbe55893f7ef82be92a4"
PROMPT_VERSION = "deadpan-hold-1"
PROMPT = ("Locked camera. The person maintains the same identity, pose, expression, "
          "and composition, with quiet minimal natural motion. No speech, no new "
          "objects, no scene change.")
EVIDENCE = Path(__file__).resolve().parent / "evidence/2026-09-20-smoke"


# Tensor layouts from the SHA-256-verified compiled v1 pack. Each digest is
# SHA-256 of the complete tensor-name -> {dtype, shape, data_offsets} mapping
# and loader metadata, encoded as sorted compact JSON. Metadata can affect
# loading (for example lora_alpha), so it stays pinned too. The exact header
# extent also pins the start of tensor data. Weights may change.
# Keep these pins in the executable worker, never in a signed data update.
TENSOR_SCHEMAS = {
    "mlx_gemma_default_text_encoder/model-00001-of-00002.safetensors": (
        171113, "1b608d2562762a5c987468794eb6ab35d46e5b23107169b856c24fd22f1b8cda", 1293),
    "mlx_gemma_default_text_encoder/model-00002-of-00002.safetensors": (
        58199, "5ca15717df114fac803ed99b41684e17e8785dbbe1f6542ca84a95b79ec82b8c", 449),
    "mlx_ltx_q4_pack/audio_vae.safetensors": (
        12236, "cd96f8d244cd2cb97348d091f1f0ac9a3dedc5099d5d9771164e2fe81b132a77", 102),
    "mlx_ltx_q4_pack/connector.safetensors": (
        40336, "7246376b34c282f841a140c0ecc3f8ce6a2cc94b7251d2cf62d59951a86c6699", 262),
    "mlx_ltx_q4_pack/ltx-2.3-22b-distilled-lora-384.safetensors": (
        513200, "eaf2fa7d46e732a5b50b67554796d37d0faaf61c636a230314032bc6bf94dfd8", 3320),
    "mlx_ltx_q4_pack/spatial_upscaler_x2_v1_1.safetensors": (
        9245, "5a9faf2ccd16b2ac8f15a4944f1f5d84f25934bae2f0e295361c1aa28654a53d", 72),
    "mlx_ltx_q4_pack/transformer-dev.safetensors": (
        1003877, "6e510b112ece563e33a75c8ee8a800423f7b07f8f0d946fa1a555f1348de86a2", 7450),
    "mlx_ltx_q4_pack/vae_decoder.safetensors": (
        10867, "b11e1bb57755a42687ebfe8b7794316e63ec7f0673338f926c077a5e58b2237a", 86),
    "mlx_ltx_q4_pack/vae_encoder.safetensors": (
        11005, "954734bb80dd8273ce1138cbafa938e8b63f6bbf01d0056b1db0f1db8bb00398", 86),
    "mlx_ltx_q4_pack/vocoder.safetensors": (
        143779, "585061229dabfc415309c6cbaf322386286a858582c0b1803fbc13919fff2a95", 1227),
}


def _tensor_schema(stream, size, expected, label):
    """Compare a bounded header with the qualified loader schema."""
    expected_length, expected_digest, expected_count = expected
    raw_length = stream.read(8)
    length = int.from_bytes(raw_length, "little")
    if (len(raw_length) != 8 or not 2 <= length <= 2 * 1024 * 1024 or
            length != expected_length or length > size - 8):
        raise ValueError(f"unsupported safetensors header extent: {label}")
    raw = stream.read(length)
    if len(raw) != length:
        raise ValueError(f"truncated safetensors header: {label}")

    def unique_fields(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate safetensors header field: {label}")
            result[key] = value
        return result

    def invalid_constant(_):
        raise ValueError(f"non-finite safetensors header value: {label}")

    header = json.loads(raw, object_pairs_hook=unique_fields, parse_constant=invalid_constant)
    if not isinstance(header, dict):
        raise ValueError(f"invalid safetensors header object: {label}")
    canonical = json.dumps(header, sort_keys=True, separators=(",", ":")).encode()
    if (len(header) - ("__metadata__" in header) != expected_count or
            hashlib.sha256(canonical).hexdigest() != expected_digest):
        raise ValueError(f"unsupported safetensors tensor schema: {label}")
    return expected_count


def _model_manifest(model_pack, model_cache, check_cancel, hash_assets):
    exact_keys(model_pack, ["pack_id", "pack_version", "model_family", "runtime_id",
                            "runtime_versions", "operations", "files"])
    if (model_pack["pack_id"] != "ltx-2.3-q4-bridge" or
            not isinstance(model_pack["pack_version"], str) or
            not re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", model_pack["pack_version"]) or
            model_pack["model_family"] != "ltx-2.3" or
            model_pack["runtime_id"] != "ltx-mlx" or
            model_pack["runtime_versions"] != ["0.15.8+deadpan1"] or
            model_pack["operations"] != ["bridge_hold"]):
        raise ValueError("unsupported bridge pack or runtime compatibility")

    baseline = json.loads((EVIDENCE / "download-manifest.json").read_text())
    expected = {}
    repository_prefixes = {
        "dgrauet/ltx-2.3-mlx-q4": "mlx_ltx_q4_pack",
        "mlx-community/gemma-3-12b-it-4bit": "mlx_gemma_default_text_encoder",
    }
    for asset in baseline["assets"]:
        prefix = repository_prefixes[asset["repository"]]
        expected[(prefix, asset["path"])] = asset

    files = model_pack["files"]
    if not isinstance(files, list) or len(files) != len(expected) or len(files) > 64:
        raise ValueError("unsupported bridge component inventory")
    if model_cache.is_symlink() or not model_cache.is_dir():
        raise ValueError("model pack root is not a regular directory")
    root_stat = model_cache.stat()
    if root_stat.st_uid != os.geteuid():
        raise ValueError("model pack root is not owned by this user")

    revisions, seen, entries = {}, set(), []
    expected_by_prefix = {prefix: set() for prefix in
                          ("mlx_ltx_q4_pack", "mlx_gemma_default_text_encoder")}
    total_bytes = 0
    for item in files:
        check_cancel()
        exact_keys(item, ["name", "sha256", "bytes"])
        name, digest, size = item["name"], item["sha256"], item["bytes"]
        if (not isinstance(name, str) or len(name) > 256 or "\\" in name or "\0" in name or
                not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest) or
                type(size) is not int or not 0 < size <= 16 * 1024**3):
            raise ValueError("malformed bridge pack file record")
        parts = name.split("/")
        if (len(parts) != 3 or any(not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", p)
                                   or p in (".", "..") for p in parts) or
                parts[0] not in ("mlx_ltx_q4_pack", "mlx_gemma_default_text_encoder") or
                not re.fullmatch(r"[0-9a-f]{40}", parts[1])):
            raise ValueError("unsafe or unsupported bridge component path")
        prefix, revision, relative = parts
        if prefix in revisions and revisions[prefix] != revision:
            raise ValueError("one bridge component names multiple revisions")
        revisions[prefix] = revision
        key = (prefix, relative)
        if key not in expected or key in seen:
            raise ValueError("unexpected or duplicate bridge component file")
        seen.add(key)
        asset = expected[key]
        expected_by_prefix[prefix].add(relative)
        # Pipeline, quantization, tensor layout, tokenizer and other config
        # remain byte-pinned to the qualified data contract. Only safetensors
        # weights and the human-readable license/readme may change.
        if (not relative.endswith(".safetensors") and relative not in ("LICENSE", "README.md") and
                (size != asset["size"] or digest != asset["sha256"])):
            raise ValueError(f"unsupported model configuration change: {relative}")
        if relative.endswith(".safetensors") and size != asset["size"]:
            raise ValueError(f"unsupported weight tensor layout or quantization size: {relative}")
        if relative == "LICENSE" and size > 256 * 1024 or relative == "README.md" and size > 1024 * 1024:
            raise ValueError(f"human-readable model asset exceeds its size bound: {relative}")
        if total_bytes > baseline["total_bytes"] - size:
            raise ValueError("bridge pack exceeds the qualified data budget")
        total_bytes += size
        entries.append((item, prefix, relative))

    if len(seen) != len(expected) or set(revisions) != {
            "mlx_ltx_q4_pack", "mlx_gemma_default_text_encoder"}:
        raise ValueError("incomplete bridge component inventory")
    model_root = model_cache / "mlx_ltx_q4_pack" / revisions["mlx_ltx_q4_pack"]
    gemma_root = model_cache / "mlx_gemma_default_text_encoder" / revisions["mlx_gemma_default_text_encoder"]
    for prefix, root in (("mlx_ltx_q4_pack", model_root),
                         ("mlx_gemma_default_text_encoder", gemma_root)):
        if root.is_symlink() or not root.is_dir():
            raise ValueError(f"unsafe or missing model component directory: {prefix}")
        if {entry.name for entry in root.iterdir()} != expected_by_prefix[prefix]:
            raise ValueError(f"unexpected files in model component directory: {prefix}")

    verified_assets = []
    for item, prefix, relative in entries:
        check_cancel()
        name, digest, size = item["name"], item["sha256"], item["bytes"]
        path = model_cache / name
        current = model_cache
        for component in name.split("/")[:-1]:
            current = current / component
            if current.is_symlink() or not current.is_dir():
                raise ValueError(f"unsafe or missing model component directory: {name}")
        metadata = os.lstat(path)
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.geteuid() or
                metadata.st_nlink != 1 or metadata.st_size != size):
            raise ValueError(f"model data missing or of the wrong type or size: {name}")
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            opened = os.fstat(descriptor)
            if (not stat.S_ISREG(opened.st_mode) or opened.st_uid != os.geteuid() or
                    opened.st_nlink != 1 or opened.st_size != size):
                raise ValueError(f"model data changed while opening: {name}")
            with os.fdopen(descriptor, "rb", closefd=False) as stream:
                if relative.endswith(".safetensors"):
                    _tensor_schema(stream, size, TENSOR_SCHEMAS[f"{prefix}/{relative}"], name)
                if hash_assets:
                    stream.seek(0)
                    digest_state, length = hashlib.sha256(), 0
                    while data := stream.read(1024 * 1024):
                        check_cancel()
                        length += len(data)
                        if length > size:
                            raise ValueError(f"model data grew beyond its manifest: {name}")
                        digest_state.update(data)
                    if length != size or digest_state.hexdigest() != digest:
                        raise ValueError(f"model data hash mismatch: {name}")
            after = os.fstat(descriptor)
            if (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns,
                    opened.st_ctime_ns) != (after.st_dev, after.st_ino, after.st_size,
                                           after.st_mtime_ns, after.st_ctime_ns):
                raise ValueError(f"model data changed while verifying: {name}")
        finally:
            os.close(descriptor)
        verified_assets.append({"repository": expected[(prefix, relative)]["repository"],
                                "path": relative, "size": size, "sha256": digest})
    return model_root, gemma_root, verified_assets


def runtime_paths(config, check_cancel, hash_assets=True, provider=None):
    exact_keys(config, ["runtime_source", "model_cache", "ffmpeg", "ffprobe", "model_pack",
                        "model_manifest_sha256"])
    paths = {key: Path(config[key]) for key in ["runtime_source", "model_cache", "ffmpeg", "ffprobe"]}
    if any(not path.is_absolute() for path in paths.values()):
        raise ValueError("runtime paths must be absolute host-selected paths")
    source_manifest = json.loads((Path(__file__).parent / "ltx-source-manifest.json").read_text())
    if source_manifest["commit"] != RUNTIME_COMMIT:
        raise ValueError("unexpected source manifest revision")
    verify_tree(paths["runtime_source"], source_manifest, check_cancel)
    verify_roots(paths["runtime_source"])
    paths["source_manifest"] = source_manifest
    if (not isinstance(config["model_manifest_sha256"], str) or
            not re.fullmatch(r"[0-9a-f]{64}", config["model_manifest_sha256"])):
        raise ValueError("selected model manifest identity is malformed")
    pack = config["model_pack"]
    if provider is not None and (
            provider.pack_id != pack.get("pack_id") or
            provider.pack_version != pack.get("pack_version") or
            provider.runtime_id != pack.get("runtime_id") or
            provider.runtime_version not in pack.get("runtime_versions", [])):
        raise ValueError("request provider differs from the selected immutable model pack")
    model, gemma, verified_assets = _model_manifest(
        pack, paths["model_cache"], check_cancel, hash_assets
    )
    paths["model_pack"] = pack
    paths["model_manifest_sha256"] = config["model_manifest_sha256"]
    paths["model"] = model
    paths["gemma"] = gemma
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
    pack = paths["model_pack"]
    report = {"schema_version": 2, "runtime_commit": RUNTIME_COMMIT,
              "adapter_sources_sha256": adapter_sources,
              "pack_id": pack["pack_id"], "pack_version": pack["pack_version"],
              "runtime_id": pack["runtime_id"],
              "runtime_version": request["provider"]["runtime_version"],
              "model_manifest_sha256": paths["model_manifest_sha256"],
              "pack_revision": paths["model"].name,
              "request_binding": {key: request[key] for key in
                                  ["identity", "project_id", "revision_id", "target", "input",
                                   "constraints", "provider", "plan"]},
              "verified_assets": paths["verified_assets"],
              "gemma_revision": paths["gemma"].name, "prompt_version": PROMPT_VERSION,
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


def check_runtime(config):
    """A model pack smoke test: the pinned runtime imports from its verified
    source, Metal runs, and all safetensors keep the qualified loader schema.
    No inference; the installer already hashed the files."""
    started = time.monotonic()
    paths = runtime_paths(config, lambda: None, hash_assets=False)
    import mlx.core as mx
    from ltx_core_mlx.text_encoders.gemma.encoders.base_encoder import GemmaLanguageModel  # noqa: F401
    from ltx_pipelines_mlx.keyframe_interpolation import KeyframeInterpolationPipeline  # noqa: F401
    sources = loaded_sources(paths["runtime_source"], paths["source_manifest"])
    values = mx.arange(1024, dtype=mx.float32)
    total = (values * values).sum().item()
    if int(total) != 357389824:
        raise ValueError("Metal arithmetic check failed")
    tensors = sum(schema[2] for schema in TENSOR_SCHEMAS.values())
    pack = paths["model_pack"]
    return {"schema_version": 1, "runtime_commit": RUNTIME_COMMIT,
            "pack_id": pack["pack_id"], "pack_version": pack["pack_version"],
            "runtime_id": pack["runtime_id"],
            "model_manifest_sha256": paths["model_manifest_sha256"],
            "device": str(mx.default_device()),
            "python": sys.version.split()[0], "mlx": mx.__version__,
            "verified_assets": len(paths["verified_assets"]), "safetensors_tensors": tensors,
            "loaded_ltx_sources": len(sources), "seconds": round(time.monotonic() - started, 3)}
