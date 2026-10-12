#!/usr/bin/env python3
"""Verify ordered-owner EQ evidence and optional current source hashes."""
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
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', log))) == 944
assert not re.search(r'test result: FAILED|test result: ok\..*; [1-9][0-9]* failed;', log)
for name in [
    'ordered_eq_matches_direct_convolution_and_mute_stays_on_its_authored_side',
    'ordered_eq_cold_cropped_group_keeps_hidden_history_and_is_partition_invariant',
    'ordered_eq_resumed_history_keeps_reference_mute_and_excludes_exterior_gain',
    'ordered_eq_cannot_fill_silent_hold_or_extend_owner_support',
    'ordered_eq_rejects_invalid_admission_and_releases_failed_reservations',
    'ordered_eq_uses_preserve_output_before_filtering_hidden_group_history',
    'ordered_eq_retained_point_clock_keeps_fractional_source_phase',
    'ordered_eq_repeat_escalation_feeds_filter_once_across_play_boundaries',
    'controlled_eq_shares_work_deadline_depth_and_retains_all_halo_dependencies',
]:
    assert name + ' ... ok' in log, name
identity = json.loads((here/'identity.json').read_text())
assert identity['test_binaries']
if args.source:
    for name, digest in identity['sources'].items():
        assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, name
print('Ordered-owner EQ evidence verified' + (' against current source' if args.source else ''))
