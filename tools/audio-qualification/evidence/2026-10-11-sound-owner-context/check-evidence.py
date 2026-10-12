#!/usr/bin/env python3
"""Verify independent sound history evidence and optional current source hashes."""
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
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', log))) == 958
assert not re.search(r'test result: FAILED|test result: ok\..*; [1-9][0-9]* failed;', log)
for name in ['sound_owner_context_restores_hidden_current_sound_and_hold_history', 'sound_owner_context_keeps_enclosing_preserve_and_scopes_repeat_input', 'sound_owner_context_keeps_fractional_current_phase_on_the_absolute_grid', 'sound_owner_context_rejects_foreign_owner_and_bounds_work_and_support', 'sound_owner_context_reads_cold_hidden_pcm_and_current_hold_gates', 'sound_owner_context_reads_full_nested_preserve_before_current_delivery', 'sound_owner_context_retains_selection_inside_the_enclosing_processor']:
    assert name + ' ... ok' in log, name
assert not re.search(r'test result: ok\..*; [1-9][0-9]* ignored;', log)
identity = json.loads((here/'identity.json').read_text())
assert identity['test_binaries']
if args.source:
    for name, digest in identity['sources'].items():
        assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, name
print('Independent sound history evidence verified' + (' against current source' if args.source else ''))
