"""Show the distant contributions omitted by a radius-256 support cutoff."""
import json
import math
from pathlib import Path

import numpy as np

import oracle

ROOT = Path(__file__).resolve().parent
results = json.loads((ROOT / "results.json").read_text())
output = []
for case in results["results"]:
    if not case["exceeds_minus_1_dbtp"]:
        continue
    channel, witness = max(enumerate(case["channels"]), key=lambda item: item[1]["refined_peak"])
    samples = oracle.load_wave(ROOT / "inputs" / (case["case"] + ".wav"))[:, channel]
    coordinate = witness["refined_coordinate"]
    integer = math.floor(coordinate)
    fraction = coordinate - integer
    indices = np.arange(len(samples))
    signs = np.where((integer - indices) % 2 == 0, 1.0, -1.0)
    terms = samples * signs * (math.sin(math.pi * fraction) / math.pi) / (coordinate - indices)
    complete = math.fsum(terms)
    supports = []
    for radius in [64, 128, 256, 512, 1024, 2048, 4096, 8192]:
        kept = math.fsum(terms[np.abs(coordinate - indices) < radius])
        supports.append({"radius": radius, "kept_unwindowed_sum": kept,
                         "omitted_signed_contribution": complete - kept})
    output.append({"case": case["case"], "channel": channel, "coordinate": coordinate,
                   "full_signed_sum": complete, "support_sums": supports})
oracle.write_json(ROOT / "support.json", output)
print(json.dumps(output, indent=2))
