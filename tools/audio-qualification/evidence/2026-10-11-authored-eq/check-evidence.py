"""Check retained authored-EQ evidence; optionally compare current source."""
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
assert [r['name'] for r in records] == ['fmt', 'tests', 'clippy', 'whitespace']
assert all(r['exit_code'] == 0 for r in records)
assert json.loads((here/'release-command.json').read_text())['exit_code'] == 0
log = gzip.decompress((here/'tests.log.gz').read_bytes()).decode()
count = sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log)))
assert count == json.loads((here/'results.json').read_text())['passed_tests'] and count > 1000
assert 'test result: FAILED' not in log
for name in [
    'authored_eq_stored_order_matches_explicit_owner_processing_and_silence',
    'authored_eq_nested_owners_keep_f64_order_hidden_crops_and_cold_partitioning',
    'authored_eq_retained_nested_history_matches_unmoved_owner_clocks',
    'authored_eq_silent_hold_masks_filter_history_and_limits_release_pcm',
    'authored_eq_whole_repeat_reuses_one_owner_across_many_play_clocks',
    'authored_eq_tail_uses_processed_input_and_shared_parent_once',
    'authored_eq_independent_sounds_fail_explicitly_before_source_admission',
    'equalizer_is_ordered_serialized_and_preserved_by_other_treatment_edits',
    'equalizer_is_authenticated_and_requires_context_schema_10',
]:
    assert name+' ... ok' in log, name
release_log = gzip.decompress((here/'release.log.gz').read_bytes()).decode()
assert 'test result: ok. 1 passed; 0 failed;' in release_log
assert 'stored_equalizer_changes_preview_and_matches_verified_export ... ok' in release_log
identity = json.loads((here/'identity.json').read_text())
release = json.loads((here/'release-identity.json').read_text())
assert release['binaries'] and identity['test_binaries']
assert release['source_identity_sha256'] == hashlib.sha256((here/'identity.json').read_bytes()).hexdigest()
assert release['movie_sha256'] == hashlib.sha256((here/'equalizer.mp4').read_bytes()).hexdigest()
report = json.loads((here/'export.json').read_text())['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked'] == 54
assert report['summary']['blocks_compared'] == 82
assert report['summary']['offset_verified_windows'] == 2
assert report['summary']['nonzero_offsets'] == 0
assert all(w['passed'] and w['offset_status'] == 'verified_zero' and w['measured_offset_samples'] == 0 for w in report['audio'])
if args.source:
    for name, digest in identity['sources'].items():
        assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, name
    assert hashlib.sha256((root/'native/deadpan-source/tests/fixtures/cfr-bframes.mp4').read_bytes()).hexdigest() == release['fixture_sha256']
print(f'Authored EQ: {count} regression tests, strict Clippy, and verified release export' +
      ('; current source matches' if args.source else ''))
