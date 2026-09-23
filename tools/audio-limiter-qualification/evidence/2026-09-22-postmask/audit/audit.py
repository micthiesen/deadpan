"""Audit retained post-mask PCM using the earlier independent finite-sinc oracle."""
import hashlib
import json
import math
import platform
import shutil
import sys
import time
from pathlib import Path

import mpmath as mp
import numpy as np

import finite_sinc

ROOT = Path(__file__).resolve().parent
SOURCE = Path('/tmp/deadpan-postmask-limiter-20260921')
CEILING = 10.0 ** (-1.0 / 20.0)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(name, data):
    (ROOT / name).write_text(json.dumps(data, indent=2, allow_nan=False) + '\n')


def precise(samples, coordinate):
    position = mp.mpf(coordinate)
    integer = int(mp.floor(position))
    phase = position - integer
    if phase == 0:
        return mp.mpf(float(samples[integer])) if 0 <= integer < len(samples) else mp.mpf(0)
    total = mp.fsum(mp.mpf(float(value)) * (-1 if n % 2 else 1) / (position - n)
                    for n, value in enumerate(samples))
    return (-1 if integer % 2 else 1) * mp.sin(mp.pi * phase) / mp.pi * total


def main():
    inputs = ROOT / 'inputs'
    inputs.mkdir(exist_ok=False)
    cases = []
    for report_name in ['results.json', 'shortened-edge-results.json']:
        shutil.copyfile(SOURCE / report_name, inputs / report_name)
        cases.extend(json.loads((inputs / report_name).read_text())['results'])
    assert len(cases) == 16
    hashes = {}
    for case in cases:
        name = case['case'] + '.wav'
        source_path = SOURCE / 'outputs' / name
        assert sha(source_path) == case['artifacts']['wav_sha256'], name
        shutil.copyfile(source_path, inputs / name)
        hashes[name] = sha(inputs / name)
        assert hashes[name] == case['artifacts']['wav_sha256']
    equality = {}
    for amplitude in ['0.1', '0.8', '16']:
        prefix = f'mask-alternating-dc-{amplitude}'
        equality[prefix] = hashes[prefix + '-hard.wav'] == hashes[prefix + '-shortened-edge.wav']
    assert all(equality.values())
    environment = {
        'python': sys.version, 'platform': platform.platform(), 'numpy': np.__version__,
        'mpmath': mp.__version__, 'argv': sys.argv, 'started_epoch_seconds': time.time(),
        'script_sha256': sha(Path(__file__)), 'oracle_sha256': sha(ROOT / 'finite_sinc.py'),
        'report_hashes': {p.name: sha(p) for p in inputs.glob('*.json')},
        'wav_hashes_verified_before_audit': hashes,
        'hard_shortened_byte_identity': equality,
        'review_provenance': 'Parent reran earlier independently authored oracle after new reviewer agents hit usage limit. No fresh independent-agent review completed.',
        'reconstruction': 'Complete finite zero-extended sinc. All samples included, no window or fixed radius.',
        'limits': 'Numerical FFT grid, analytic tail/curvature bounds evaluated in float64, and 80-digit witnesses. Not an interval certificate or universal qualification.',
    }
    write('environment.json', environment)
    mp.mp.dps = 80
    results, witnesses, cached = [], [], {}
    started = time.monotonic()
    for case in cases:
        name = case['case']
        path = inputs / (name + '.wav')
        pcm = finite_sinc.load_wave(path)
        assert float(np.max(np.abs(pcm))) == case['sample_peak']
        masked = name.startswith('mask-alternating-dc-')
        if masked:
            assert np.all(pcm[1::2] == 0)
            assert np.all(np.any(pcm[0::2] != 0, axis=1))
        channels = []
        for channel in range(2):
            samples = pcm[:, channel]
            identity = hashlib.sha256(samples.tobytes()).hexdigest()
            reused = identity in cached
            if not reused:
                l1 = math.fsum(np.abs(samples))
                required = max(2, math.ceil(l1 / (math.pi * CEILING)) + 1)
                distance = 1 << (required - 1).bit_length()
                report = finite_sinc.audit_channel(samples, 128, distance, 16)
                value = precise(samples, report['refined_coordinate'])
                assert abs(float(value) - report['refined_signed_value']) < 1e-12
                witness = {
                    'coordinate_hex': report['refined_coordinate'].hex(),
                    'coordinate_decimal': mp.nstr(mp.mpf(report['refined_coordinate']), 80),
                    'signed_value': mp.nstr(value, 75), 'magnitude': mp.nstr(abs(value), 75),
                    'dbtp': mp.nstr(20 * mp.log10(abs(value)), 75) if value else None,
                    'exceeds_minus_1_dbtp': bool(abs(value) > mp.power(10, -mp.mpf(1) / 20)),
                }
                cached[identity] = (report, witness, name, channel)
            report, witness, original_case, original_channel = cached[identity]
            channels.append(dict(report, channel=channel, identical_channel_reused=reused,
                                 original_case=original_case, original_channel=original_channel))
            witnesses.append(dict(witness, case=name, channel=channel, wav_sha256=hashes[path.name]))
        result = {
            'case': name, 'wav_sha256': hashes[path.name], 'frames': len(pcm),
            'masked_odd_frames_verified_zero': 4096 if masked else None,
            'active_even_frames_nonzero': 4096 if masked else None,
            'channels': channels,
            'worst_refined_dbtp': max(c['refined_dbtp'] for c in channels),
            'exceeds_minus_1_dbtp': any(c['exceeds_minus_1_dbtp'] for c in channels),
            'qualified_numerical_global_upper': max(c['qualified_numerical_global_upper'] for c in channels),
        }
        results.append(result)
        write('results.json', {'environment': environment, 'results': results, 'seconds': time.monotonic() - started})
        write('high-precision.json', {'decimal_digits': 80, 'checks': witnesses})
        print(json.dumps({k: result[k] for k in ['case', 'worst_refined_dbtp', 'exceeds_minus_1_dbtp', 'qualified_numerical_global_upper']}), flush=True)
    assert all(sha(inputs / name) == expected for name, expected in hashes.items())
    write('summary.json', {'case_count': len(results), 'unique_channels': len(cached),
                           'failures': [r['case'] for r in results if r['exceeds_minus_1_dbtp']],
                           'all_input_hashes_verified_after_audit': True})


if __name__ == '__main__':
    main()
