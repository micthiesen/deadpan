#!/usr/bin/env python3
"""Audit finite wet-effect results, retained bytes and optional current source."""
import argparse
import gzip
import hashlib
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--source', action='store_true')
args = parser.parse_args()
here = Path(__file__).resolve().parent
repo = here.parents[3]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(name):
    return json.loads((here / name).read_text())

def log(name):
    return gzip.decompress((here / (name + '.log.gz')).read_bytes()).decode()

manifest = read('manifest.json')
assert set(manifest) == {p.name for p in here.iterdir() if p.is_file() and p.name != 'manifest.json'}
for name, digest in manifest.items():
    assert sha(here / name) == digest, name
commands = {row['name']: row for row in read('commands.json')}
for name, row in commands.items():
    assert hashlib.sha256(log(name).encode()).hexdigest() == row['log_sha256'], name
final = ['audio-dsp-tests-final', 'clippy-final', 'dsp-final', 'wet-final',
         'wet-release-build', 'fmt', 'whitespace', 'causal-abi-build-final',
         'causal-abi-sanitizer-build-final', 'causal-abi-final',
         'causal-abi-sanitized-final', 'wet-probe-1', 'wet-probe-2', 'wet-probe-3']
for name in final:
    assert commands[name]['exit'] == 0, name
    assert not re.search(r'runtime error:|ERROR: AddressSanitizer|test result: FAILED', log(name)), name
    assert not re.search(r'; [1-9][0-9]* (?:failed|ignored);', log(name)), name
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed', log('audio-dsp-tests-final')))) == 622
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed', log('dsp-final')))) == 32
assert '7 passed; 0 failed; 0 ignored;' in log('wet-final')
# Keep earlier failures visible. The first sanitizer process exited zero but
# emitted a UB diagnostic; it is a failed check, never counted as a pass.
assert 'runtime error: constructor call on misaligned address' in log('causal-abi-sanitized')
assert commands['audio-dsp-tests']['exit'] == 101
assert commands['clippy']['exit'] == 101
for name in ['causal-abi-final', 'causal-abi-sanitized-final']:
    report = read(name + '.json')
    assert report == json.loads(log(name))
    assert report['creation_allocation_failure_sites'] == 112
    assert report['processed_tiles'] == 96
    assert report['processing_allocations'] == report['leaked_allocations'] == 0
    assert report['guards'] == 'passed'
    assert len(report['cases']) == 8
    for case in report['cases']:
        assert 0 < case['native_resident_bytes'] <= case['native_peak_bytes'] <= case['admission_bytes']
    assert report['cases'][-1]['taps'] == 384001
    assert report['cases'][-1]['native_peak_bytes'] == 47252000
identity = read('identity.json')
reference = None
for number in [1, 2, 3]:
    name = f'wet-probe-{number}'
    report = read(name + '.json')
    assert report == json.loads(log(name))
    assert commands[name]['binaries']['target/release/examples/finite_wet_probe'] == identity['binaries']['target/release/examples/finite_wet_probe']
    assert len(report['cases']) == 2
    hashes = []
    for case, effect, history in zip(report['cases'], ['reverb', 'delay'], [192000, 384000]):
        assert case['effect'] == effect
        assert case['history_frames'] == case['tail_frames'] == history
        assert case['latency_frames'] == 0 and case['tile_frames'] == 4096
        assert case['maximum_direct_error'] < 2e-12
        assert case['cold_warm_bit_identical'] and case['measured_tiles'] == 101
        assert case['direct_samples_compared'] == 20
        assert 0 < case['tile_p50_ns'] <= case['tile_p99_ns'] <= case['tile_max_ns']
        hashes.append(tuple(case[k] for k in ['kernel_sha256_planar_f64le', 'input_sha256_planar_f64le', 'output_sha256_planar_f64le']))
    if reference is not None:
        assert hashes == reference
    reference = hashes
assert commands['audio-dsp-tests-final']['binaries'] and commands['dsp-final']['binaries']
if args.source:
    for name, digest in identity['sources'].items():
        assert sha(repo / name) == digest, name
print('Finite wet-effect evidence verified' + (' against current source' if args.source else ''))
