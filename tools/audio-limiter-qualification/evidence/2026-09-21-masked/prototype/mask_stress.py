"""Additional adversarial mask probe; leaves the original 42 outputs intact."""
import json
from pathlib import Path
import time

import numpy as np
import prototype as p

out = Path('/tmp/deadpan-joint-limiter-20260921/mask-stress')
out.mkdir(exist_ok=False)
c = np.fromfile(p.CONDITIONER_ROOT / 'kaiser255-beta16.f64le', dtype='<f8')
table = np.array(json.loads((p.CONDITIONER_ROOT / 'combined-kernels.json').read_text())['bs1770_table_rows_oldest_to_newest'])
mask = np.arange(p.FRAMES) % 2 == 0
started = time.monotonic()
constraints = p.constraint_set(c, mask, table)
results = []
for amplitude in [.1, .8, 16]:
    samples = np.full((p.FRAMES, 2), amplitude, dtype=np.float32).astype(np.float64)
    y, gain, margin = p.limit(samples, c, mask, constraints, 8192)
    path = out / f'alternating-mask-dc-{amplitude}.wav'
    p.write_wave(path, y)
    results.append({'case': path.stem, 'output_sha256': p.sha(path), 'sample_peak': float(abs(y).max()), 'gain_min': float(gain.min()), 'gain_max': float(gain.max()), 'margin_min': margin})
    print(results[-1], flush=True)
(out / 'results.json').write_text(json.dumps({'results': results, 'seconds': time.monotonic() - started}, indent=2) + '\n')
