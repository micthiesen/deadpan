#!/usr/bin/env python3
"""Validate the retained input-automation results, optionally against source."""
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

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(name):
    return json.loads((here / name).read_text())

def log(name):
    return gzip.decompress((here / (name + '.log.gz')).read_bytes()).decode()

manifest = read('manifest.json')
assert set(manifest) == {p.name for p in here.iterdir() if p.is_file() and p.name != 'manifest.json'}
for name, expected in manifest.items():
    assert digest(here / name) == expected, name
for record in read('commands.json'):
    assert record['exit'] == (101 if record['name'] == 'baseline' else 0), record
    assert hashlib.sha256(log(record['name']).encode()).hexdigest() == record['log_sha256']
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', log('suites')))) == 973
assert not re.search(r'test result: FAILED|; [1-9][0-9]* (?:failed|ignored);', log('suites'))
assert 'test result: ok. 7 passed; 0 failed; 0 ignored;' in log('final-focus')
assert 'test result: FAILED. 4 passed; 3 failed; 0 ignored;' in log('baseline')
regressions = [
    'independent_pre_saturation_mute_preserves_each_input_clock',
    'independent_pre_saturation_envelope_preserves_each_input_clock',
    'shared_eq_independent_input_automation_keeps_gain_filter_and_saturation_order',
]
for name in regressions:
    assert name + ' ... FAILED' in log('baseline'), name
    assert name + ' ... ok' in log('final-focus'), name
assert 'divergent_post_saturation_automation_still_refuses_before_source_access ... ok' in log('final-focus')
assert 'shared_equalizer_preserves_mixed_preview_and_verified_export ... ok' in log('export')
report = read('export-results.json')['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked'] == 54
assert report['summary']['blocks_compared'] == 83
assert len(report['audio']) == 2
assert all(w['passed'] and w['offset_status'] == 'verified_zero' and w['measured_offset_samples'] == 0 for w in report['audio'])
receipt = read('render-receipt.json')
assert receipt['destination_readback']['sha256'] == digest(here / 'shared-equalizer.mp4')
assert receipt['destination_readback']['bytes'] == (here / 'shared-equalizer.mp4').stat().st_size
assert receipt['provenance']['document_sha256'] == report['document_sha256']
identity = read('identity.json')
baseline = read('baseline-source.json')
assert baseline['revision'] == identity['base_commit']
assert read('baseline-binaries.json') and identity['binaries']['final-focus'] and identity['binaries']['export']
assert '/baseline-target/' in read('baseline-binaries.json')[0]['path']
for name, expected in baseline['overlay'].items():
    assert identity['sources'][name] == expected
if args.source:
    for name, expected in identity['sources'].items():
        assert digest(repo / name) == expected, name
print('Input automation evidence verified' + (' against current source' if args.source else ''))
