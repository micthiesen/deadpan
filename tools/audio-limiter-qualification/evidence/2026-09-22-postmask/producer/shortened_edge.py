"""Separate tiny-fragment endpoint candidate over the retained prototype.

The first run's fixed96 mode is intentionally left byte-for-byte unchanged.
This candidate shortens a linear edge fade to half the active run length and
evaluates weights at sample centers.  It is an experiment, not product policy.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import time

import numpy as np


ROOT = Path(__file__).resolve().parent
PROTOTYPE_PATH = ROOT / "prototype.py"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_prototype():
    spec = importlib.util.spec_from_file_location("retained_postmask_prototype", PROTOTYPE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load retained prototype")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def shortened_edge(mask: np.ndarray, mode: str) -> np.ndarray:
    if mode != "shortened-edge-candidate":
        raise ValueError(mode)
    envelope = np.zeros(len(mask), dtype=np.float64)
    at = 0
    while at < len(mask):
        if not mask[at]:
            at += 1
            continue
        end = at + 1
        while end < len(mask) and mask[end]:
            end += 1
        length = end - at
        fade = min(96.0, length / 2)
        positions = np.arange(length, dtype=np.float64)
        envelope[at:end] = np.minimum(
            1.0,
            np.minimum((positions + 0.5) / fade, (length - positions - 0.5) / fade),
        )
        at = end
    return envelope


def main() -> None:
    p = load_prototype()
    manifest = {
        "schema_version": 1,
        "captured_before_shortened_edge_outputs": True,
        "utc_epoch_seconds": time.time(),
        "hashes": {
            "shortened_edge.py": sha256(Path(__file__)),
            "retained_prototype.py": sha256(PROTOTYPE_PATH),
            "kaiser255-beta16.f64le": sha256(p.COEFF_PATH),
            "combined-kernels.json": sha256(p.KERNELS_PATH),
            "measure_pcm": sha256(p.METER),
        },
    }
    manifest_path = ROOT / "shortened-edge-pre-run-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")

    conditioner = np.fromfile(p.COEFF_PATH, dtype="<f8")
    table = np.array(json.loads(p.KERNELS_PATH.read_text())["bs1770_table_rows_oldest_to_newest"])
    rows = p.finite_rows(table)
    n = np.arange(p.FRAMES)
    mask = n % 2 == 0
    cases = [
        {
            "name": f"mask-alternating-dc-{amplitude:g}-shortened-edge",
            "input": np.full((p.FRAMES, 2), amplitude),
            "mask": mask,
            "mode": "shortened-edge-candidate",
            "policy": "sample-centered fade span min(96, run_length/2); one-sample active island weight 1",
        }
        for amplitude in (0.1, 0.8, 16.0)
    ]
    old_boundary = p.boundary_envelope
    p.boundary_envelope = shortened_edge
    report = {
        "schema_version": 1,
        "status": "running",
        "claim": "separate shortened-edge candidate, not predetermined product policy or qualification",
        "endpoint_rule": "f=min(96,N/2); w[i]=min(1,(i+0.5)/f,(N-i-0.5)/f)",
        "one_sample_weight": 1.0,
        "two_sample_weights": [0.5, 0.5],
        "retained_fixed96_outputs_modified": False,
        "pre_run_manifest": str(manifest_path),
        "results": [],
        "failures": [],
    }
    output_path = ROOT / "shortened-edge-results.json"
    output_path.write_text(json.dumps(report, indent=2) + "\n")
    try:
        for case in cases:
            try:
                result = p.run_case(case, conditioner, rows)
                report["results"].append(result)
                print(
                    result["case"],
                    f"BS={result['official_bs1770_meter']['dbtp']:.6f}",
                    f"K8={result['independent_finite_detector']['dbtp']:.6f}",
                    f"gain={result['gain_min']:.9f}..{result['gain_max']:.9f}",
                    flush=True,
                )
            except Exception as error:
                report["failures"].append(
                    {"case": case["name"], "type": type(error).__name__, "message": str(error)}
                )
                print(case["name"], "FAILED", repr(error), flush=True)
            output_path.write_text(json.dumps(report, indent=2) + "\n")
    finally:
        p.boundary_envelope = old_boundary
    report["status"] = "complete_with_failures" if report["failures"] else "complete"
    report["all_pass_both_retained_finite_detectors"] = bool(report["results"]) and all(
        item["passes_both_retained_finite_detectors"] for item in report["results"]
    ) and not report["failures"]
    report["all_one_sample_weights_unity"] = all(
        item["minimum_boundary_envelope_on_active"] == 1.0 for item in report["results"]
    )
    output_path.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
