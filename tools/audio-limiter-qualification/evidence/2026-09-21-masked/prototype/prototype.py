"""Scratch experiment: shared gain, fixed conditioner, exact output masks.

Not a production limiter. Includes mask-dependent finite reconstruction kernels
in its gain bounds and emits exact float32 fixtures for independent inspection.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import time

import numpy as np

RATE = 48000
FRAMES = 8192
RADIUS = 256
CEILING = 10 ** (-1.25 / 20) - 1e-6
ROOT = Path(__file__).resolve().parent
CONDITIONER_ROOT = Path('/tmp/deadpan-conditioner-20260921')
REPO = Path('/Users/michael/Code/deadpan')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def interpolation(phases):
    offsets = np.arange(1 - RADIUS, RADIUS + 1)
    kernels = []
    for phase in range(phases):
        distance = phase / phases - offsets
        window = np.i0(10 * np.sqrt(np.maximum(0, 1 - (distance / RADIUS) ** 2))) / np.i0(10)
        weights = np.where(abs(distance) < RADIUS, np.sinc(distance) * window, 0)
        weights /= sum(weights)
        if phase == 0:
            weights[:] = 0
            weights[RADIUS - 1] = 1
        kernels.append(weights)
    return offsets, kernels


class Constraint:
    def __init__(self, name, offsets, weights, conditioner, mask):
        self.name = name
        self.times = np.arange(-RADIUS, len(mask) + RADIUS)
        radius = len(conditioner) // 2
        self.offsets = np.arange(offsets[0] - radius, offsets[-1] + radius + 1)
        self.weights = np.convolve(weights, conditioner)
        self.distance_weights = abs(self.weights) * abs(self.offsets)
        self.left = int(-self.times[0] - self.offsets[0])
        self.right = int(self.times[-1] + self.offsets[-1] - (len(mask) - 1))
        prefix = np.concatenate(([0], np.cumsum(mask, dtype=np.int64)))
        lo = np.clip(self.times + offsets[0], 0, len(mask))
        hi = np.clip(self.times + offsets[-1] + 1, 0, len(mask))
        counts = prefix[hi] - prefix[lo]
        self.zero = counts == 0
        self.partial = np.flatnonzero((counts > 0) & (counts < len(offsets)))
        special = []
        for index in self.partial:
            output_indices = self.times[index] + offsets
            active = (output_indices >= 0) & (output_indices < len(mask))
            active &= mask[np.clip(output_indices, 0, len(mask) - 1)]
            special.append(np.convolve(weights * active, conditioner))
        self.special = np.array(special).reshape(-1, len(self.offsets))
        self.special_distance = abs(self.special) * abs(self.offsets)
        self.max_distance = max(float(sum(self.distance_weights)), float(np.max(np.sum(self.special_distance, axis=1), initial=0)))

    def values(self, samples):
        padded = np.pad(samples, ((self.left, self.right), (0, 0)))
        value = np.column_stack([np.correlate(padded[:, channel], self.weights, 'valid') for channel in range(2)])
        distance = np.column_stack([np.correlate(abs(padded[:, channel]), self.distance_weights, 'valid') for channel in range(2)])
        if len(self.partial):
            windows = np.lib.stride_tricks.sliding_window_view(padded, len(self.offsets), axis=0)[self.partial]
            value[self.partial] = np.einsum('ij,icj->ic', self.special, windows)
            distance[self.partial] = np.einsum('ij,icj->ic', self.special_distance, abs(windows))
        value[self.zero] = 0
        distance[self.zero] = 0
        return value, distance


def constraint_set(conditioner, mask, table):
    constraints = [Constraint('output-samples', np.array([0]), np.array([1.0]), conditioner, mask)]
    constraints += [Constraint(f'bs1770-{p}', np.arange(-6, 6), table[:, p], conditioner, mask) for p in range(4)]
    offsets, kernels = interpolation(8)
    # Phase zero duplicates the direct-output constraint.
    constraints += [Constraint(f'kaiser256-{p}', offsets, kernels[p], conditioner, mask) for p in range(1, 8)]
    return constraints


def limit(samples, conditioner, mask, constraints, attack):
    bounds = np.ones(len(samples) + 2 * RADIUS)
    smallest_margin = math.inf
    for constraint in constraints:
        value, distance = constraint.values(samples)
        margin = CEILING - distance / attack
        smallest_margin = min(smallest_margin, float(np.min(margin)))
        if smallest_margin <= 0:
            raise ValueError(f'nonpositive margin: {smallest_margin}')
        allowed = np.divide(margin, abs(value), out=np.ones_like(value), where=value != 0)
        bounds = np.minimum(bounds, np.min(allowed, axis=1))
    future = bounds.copy()
    for i in range(len(future) - 2, -1, -1):
        future[i] = min(future[i], future[i + 1] + 1 / attack)
    release = max(4800, attack)
    gain = future.copy()
    for i in range(1, len(gain)):
        gain[i] = min(gain[i], gain[i - 1] + 1 / release)
    assert np.min(gain) >= 0 and np.max(gain) <= 1
    assert np.max(abs(np.diff(gain))) <= 1 / attack + 1e-12
    actual_gain = gain[RADIUS:RADIUS + len(samples)]
    controlled = samples * actual_gain[:, None]
    output = np.column_stack([np.convolve(controlled[:, channel], conditioner, 'same') for channel in range(2)])
    output *= mask[:, None]
    output = output.astype(np.float32)
    assert np.all(output[~mask] == 0)
    return output, actual_gain, smallest_margin


def write_wave(path, pcm):
    data = pcm.astype('<f4').tobytes()
    header = struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', 36 + len(data), b'WAVE', b'fmt ', 16, 3, 2, RATE, RATE * 8, 8, 32, b'data', len(data))
    path.write_bytes(header + data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--attack', type=int, default=8192)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    conditioner_path = CONDITIONER_ROOT / 'kaiser255-beta16.f64le'
    conditioner = np.fromfile(conditioner_path, dtype='<f8')
    table = np.array(json.loads((CONDITIONER_ROOT / 'combined-kernels.json').read_text())['bs1770_table_rows_oldest_to_newest'])
    assert len(conditioner) == 255
    assert sha(conditioner_path) == '8b4418568c73525050f7541cd4b8c8f8f4be8318e0f314260b63575d4aa3ce12'
    mask = np.ones(FRAMES, dtype=bool)
    geometries = {'full': constraint_set(conditioner, mask, table)}
    masks = {'full': mask}
    for name, spans in [('gap', [(3072, 3584)]), ('tiny-gaps', [(2048, 2049), (4096, 4098), (6000, 6017)]), ('silent', [(0, FRAMES)])]:
        m = mask.copy()
        for start, end in spans:
            m[start:end] = False
        masks[name] = m
        geometries[name] = constraint_set(conditioner, m, table)
    t = np.arange(FRAMES)
    cases = []
    for frequency in [200, 3000, 12000, 21600, 23000, 23990]:
        for amplitude in [.1, .86, 1, 4, 16]:
            mono = amplitude * np.sin(2 * np.pi * frequency * (t + .5) / RATE)
            cases.append((f'tone-{frequency}-{amplitude}', np.column_stack((mono, mono * .25)), 'full'))
    rng = np.random.default_rng(682301)
    cases.append(('random16', rng.uniform(-16, 16, (FRAMES, 2)), 'full'))
    for magnitude in [.95, 16]:
        cases.append((f'alternating-{magnitude}', np.repeat((magnitude * (-1.) ** t)[:, None], 2, axis=1), 'full'))
    impulses = np.zeros((FRAMES, 2)); impulses[FRAMES // 2] = [16, -4]
    cases.append(('impulse16', impulses, 'full'))
    two = np.zeros((FRAMES, 2)); two[FRAMES // 2:FRAMES // 2 + 2] = 1
    cases.append(('two-positive-full-scale-samples', two, 'full'))
    edges = np.zeros((FRAMES, 2)); edges[:7] = 16; edges[-3:] = -16
    cases.append(('edge-burst', edges, 'full'))
    for name in ['gap', 'tiny-gaps', 'silent']:
        cases.append((f'masked-{name}-random16', rng.uniform(-16, 16, (FRAMES, 2)), name))
        mono = 16 * np.sin(2 * np.pi * 23990 * (t + .5) / RATE)
        cases.append((f'masked-{name}-23990', np.column_stack((mono, -mono)), name))
    report = {'schema_version': 1, 'numpy': np.__version__, 'attack': args.attack, 'release': max(4800, args.attack), 'internal_dbtp': -1.25, 'conditioner_sha256': sha(conditioner_path), 'script_sha256': sha(Path(__file__)), 'bounds': {name: {c.name: {'max_distance': c.max_distance, 'partial_rows': len(c.partial)} for c in constraints} for name, constraints in geometries.items()}, 'results': []}
    start_time = time.monotonic()
    for name, samples, mask_name in cases:
        tick = time.monotonic()
        x = samples.astype(np.float32).astype(np.float64)
        output, gain, margin = limit(x, conditioner, masks[mask_name], geometries[mask_name], args.attack)
        write_wave(args.output / (name + '.wav'), output)
        raw = args.output / (name + '.f32le'); output.astype('<f4').tofile(raw)
        meter_path = args.output / (name + '-meter.json')
        subprocess.run([str(REPO / 'target/release/examples/measure_pcm'), str(raw), str(meter_path)], check=True, capture_output=True)
        meter = json.loads(meter_path.read_text())
        peak = max(meter['peaks']['true_peak'])
        item = {'case': name, 'mask': mask_name, 'sample_peak': float(abs(output).max()), 'bs1770_true_peak': peak, 'bs1770_dbtp': 20 * math.log10(peak) if peak else None, 'passes_bs1770_ceiling': peak <= 10 ** (-1 / 20), 'gain_min': float(gain.min()), 'gain_max': float(gain.max()), 'gain_all_unity': bool(np.all(gain == 1)), 'margin_min': margin, 'output_sha256': sha(args.output / (name + '.wav')), 'seconds': time.monotonic() - tick}
        report['results'].append(item)
        (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
        print(name, item['bs1770_dbtp'], item['passes_bs1770_ceiling'], 'gain', item['gain_min'], item['gain_max'], flush=True)
    report['seconds'] = time.monotonic() - start_time
    (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
