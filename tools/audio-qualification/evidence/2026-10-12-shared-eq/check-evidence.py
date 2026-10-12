#!/usr/bin/env python3
"""Verify shared EQ evidence and optionally the current implementation."""
import argparse, gzip, hashlib, json, re
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--source',action='store_true');args=parser.parse_args()
here=Path(__file__).resolve().parent;root=here.parents[3]
for name,digest in json.loads((here/'manifest.json').read_text()).items():
 assert hashlib.sha256((here/name).read_bytes()).hexdigest()==digest,name
records=json.loads((here/'commands.json').read_text());assert all(r['exit_code']==0 for r in records)
assert [r['name'] for r in records]==['tests','fmt','clippy','release','whitespace']
log=gzip.decompress((here/'tests.log.gz').read_bytes()).decode()
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log)))==970
assert not re.search(r'test result: FAILED|; [1-9][0-9]* (?:failed|ignored);',log)
for name in ['shared_eq_saturation_processes_combined_voices_in_authored_order','shared_eq_nested_owners_match_explicit_shared_prefix','shared_eq_allowed_sound_cannot_release_forbidden_original_or_peer_history','shared_eq_crop_keeps_complete_attachment_history_and_cold_partitioning','shared_eq_limit_and_cancellation_release_residency','shared_eq_saved_crop_neutral_recipe_preserves_current_delivery_edges','shared_eq_saved_copied_crop_filters_complete_history_and_rechecks_sources','authored_eq_independent_root_sound_bypasses_child_equalizer']:
 assert name+' ... ok' in log,name
release=gzip.decompress((here/'release.log.gz').read_bytes()).decode()
assert 'shared_equalizer_preserves_mixed_preview_and_verified_export ... ok' in release
assert 'test result: ok. 1 passed;' in release
report=json.loads((here/'export.json').read_text())['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked']==54
assert all(w['passed'] and w['measured_offset_samples']==0 for w in report['audio'])
identity=json.loads((here/'identity.json').read_text());assert identity['test_binaries']
assert json.loads((here/'release-identity.json').read_text())
if args.source:
 for name,digest in identity['sources'].items():
  assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest,name
print('Shared EQ evidence verified'+(' against current source' if args.source else ''))
