#!/usr/bin/env python3
"""Verify saved beat-sound input evidence and optional current source hashes."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re

parser = argparse.ArgumentParser()
parser.add_argument('--source', action='store_true')
args = parser.parse_args()
here = Path(__file__).resolve().parent
root = here.parents[3]
for name, digest in json.loads((here/'manifest.json').read_text()).items():
    assert hashlib.sha256((here/name).read_bytes()).hexdigest() == digest, name
records = json.loads((here/'commands.json').read_text())
assert all(record['exit_code'] == 0 for record in records)
assert [record['name'] for record in records] == ['fmt', 'tests', 'clippy', 'whitespace']
log = gzip.decompress((here/'tests.log.gz').read_bytes()).decode()
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', log))) == 963
assert not re.search(r'test result: FAILED|test result: ok\..*; [1-9][0-9]* failed;', log)
for name in ['sound_owner_input_keeps_saved_crop_clock_after_fractional_move_and_copy', 'sound_owner_input_preserves_nested_saved_preserve_and_current_edges', 'sound_owner_input_admits_limits_before_media_and_recovers_after_failure', 'sound_owner_input_saved_hidden_history_keeps_current_hold_fades', 'sound_owner_context_reads_cold_hidden_pcm_and_current_hold_gates', 'sound_owner_context_reads_full_nested_preserve_before_current_delivery', 'sound_owner_context_retains_selection_inside_the_enclosing_processor', 'sound_owner_context_identity_route_preserves_delivery_anchor_and_rejects_windows', 'root_capture_rejects_crops_resumes_and_equal_count_grid_substitutions']:
    assert name + ' ... ok' in log, name
assert not re.search(r'test result: ok\..*; [1-9][0-9]* ignored;', log)
before = gzip.decompress((here/'before-anchor.log.gz').read_bytes()).decode()
assert 'sound_owner_input_keeps_saved_crop_clock_after_fractional_move_and_copy ... FAILED' in before
assert json.loads((here/'before-anchor-identity.json').read_text())['binaries']
identity = json.loads((here/'identity.json').read_text())
assert identity['test_binaries']
if args.source:
    for name, digest in identity['sources'].items():
        assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, name
print('Saved beat-sound input evidence verified' + (' against current source' if args.source else ''))
