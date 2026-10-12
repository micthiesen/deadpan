#!/usr/bin/env python3
"""Check retained shared-automation measurements and optional source pins."""
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
commands = {record['name']: record for record in read('commands.json')}
for name, record in commands.items():
    assert hashlib.sha256(log(name).encode()).hexdigest() == record['log_sha256'], name
for name in ['suites-final2', 'focus-final', 'clippy-verified', 'fmt-verified', 'whitespace-verified', 'export-source']:
    assert commands[name]['exit'] == 0, name
assert commands['baseline-final']['exit'] == 101
results = read('results.json')
assert results['suite_passed'] == 1069
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', log('suites-final2')))) == results['suite_passed']
assert not re.search(r'test result: FAILED|; [1-9][0-9]* (?:failed|ignored);', log('suites-final2'))
assert 'test result: ok. 15 passed; 0 failed; 0 ignored;' in log('focus-final')
assert results['baseline_passed'] + results['baseline_expected_failures'] == 15
assert results['baseline_expected_failures'] >= 6
assert f"test result: FAILED. {results['baseline_passed']} passed; {results['baseline_expected_failures']} failed; 0 ignored;" in log('baseline-final')
for name in ['independent_post_saturation_mute_preserves_the_shared_drive',
    'independent_post_saturation_envelope_preserves_the_shared_drive',
    'independent_post_shared_automation_preserves_every_eq_order',
    'independent_post_saturation_inputs_rejoin_the_parent_bus',
    'cancelling_inputs_keep_the_shared_saturation_zero_limit_before_mute',
    'independent_automation_is_invariant_across_owner_activation_boundaries']:
    assert name + ' ... FAILED' in log('baseline-final'), name
    assert name + ' ... ok' in log('focus-final'), name
assert 'Limit("EQ owner count")' in log('context-limit-before2')
assert 'independent_clocks_share_one_eq_processing_context ... ok' in log('focus-final')
assert 'post_shared_automation_preserves_preview_and_verified_export ... ok' in log('export-source')
export = read('export-results.json')
report = export['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked'] == 31
assert report['audio']
assert all(w['passed'] and w['offset_status'] == 'verified_zero' and w['measured_offset_samples'] == 0 for w in report['audio'])
assert any('post-EQ gain differs' in note for note in export['notes'])
receipt = read('render-receipt.json')
assert receipt['destination_readback']['sha256'] == digest(here / 'shared-automation.mp4')
assert receipt['destination_readback']['bytes'] == (here / 'shared-automation.mp4').stat().st_size
assert receipt['provenance']['document_sha256'] == report['document_sha256']
identity = read('identity.json')
baseline = read('baseline-source.json')
assert baseline['revision'] == identity['base_commit']
assert read('baseline-binaries.json') and identity['binaries']['focus-final'] and identity['binaries']['export-source']
assert all('/baseline-target/' in path for path in read('baseline-binaries.json'))
for name, expected in baseline['overlay'].items():
    assert identity['sources'][name] == expected, name
if args.source:
    for name, expected in identity['sources'].items():
        assert digest(repo / name) == expected, name
print('Shared automation evidence verified' + (' against current source' if args.source else ''))
