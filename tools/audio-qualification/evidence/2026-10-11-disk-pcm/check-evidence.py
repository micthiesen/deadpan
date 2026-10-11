from pathlib import Path
import gzip
import hashlib
import json
import re

root = Path(__file__).resolve().parent
repo = root.parents[3]

def read(name):
    path = root / name
    return json.loads(gzip.decompress(path.read_bytes()) if path.suffix == '.gz' else path.read_text())

for name, meta in read('manifest.json').items():
    data = (root / name).read_bytes()
    assert len(data) == meta['bytes'] and hashlib.sha256(data).hexdigest() == meta['sha256'], name
for path, digest in read('source-pins.json')['files'].items():
    assert hashlib.sha256((repo / path).read_bytes()).hexdigest() == digest, path
for path in root.glob('*.log.gz'):
    receipt = read(path.name.removesuffix('.log.gz') + '.json')
    assert hashlib.sha256(gzip.decompress(path.read_bytes())).hexdigest() == receipt['log_sha256'], path
for name in ['check', 'storage-tests', 'disk-tests', 'suites', 'clippy', 'clippy-final',
             'disk-quota-final', 'export-final', 'format-check']:
    assert read(name + '.json')['exit'] == 0, name
totals = [tuple(map(int, match)) for match in re.findall(
    r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',
    gzip.decompress((root / 'suites.log.gz').read_bytes()).decode(),
)]
assert all(failed == ignored == 0 for _, failed, ignored in totals)
assert sum(row[0] for row in totals) == 1621
final = gzip.decompress((root / 'disk-quota-final.log.gz').read_bytes()).decode()
assert 'disk_stage_matches_memory_and_releases_evicted_storage ... ok' in final
assert 'test result: ok. 1 passed;' in final
old = gzip.decompress((root / 'baseline.log.gz').read_bytes()).decode()
assert read('baseline.json')['exit'] == 101
assert old.count('test result: FAILED. 0 passed; 1 failed;') == 1
assert 'at most 175 seconds output' in old
first_export = gzip.decompress((root / 'export-release.log.gz').read_bytes()).decode()
assert read('export-release.json')['exit'] == 101
assert 'disk-preserve frame 1 was not compared' in first_export
report = read('export.json.gz')['report']
summary = report['summary']
assert report['passed'] and not report['failures']
assert summary['pictures_checked'] == 7200 and summary['pictures_failed'] == 0
assert report['movie']['expected_audio_samples'] == 11_531_520
assert summary['audio_windows_failed'] == summary['nonzero_offsets'] == 0
assert summary['audio_windows_checked'] == 240
assert summary['offset_verified_windows'] == summary['signal_windows'] == 120
assert summary['offset_unobservable_windows'] == 0
end = 0
for window in report['audio']:
    assert window['passed'] and not window['flags'] and window['uncovered_samples'] == 0
    assert window['window'][0] == end
    end = window['window'][1]
    if window['kind'] == 'signal':
        assert window['offset_status'] == 'verified_zero' and window['measured_offset_samples'] == 0
    else:
        assert window['kind'] == 'silent' and window['offset_status'] == 'not_applicable'
        assert window['measured_offset_samples'] is None
assert end == report['movie']['expected_audio_samples']
assert all(picture['passed'] for picture in report['pictures'])
for frame in [0, 1, 2, 58, 59, 60, 3599, 3600, 7199]:
    picture = next(p for p in report['pictures'] if p['output_frame'] == frame)
    assert picture['provenance']['source_frame'] == 12 + (frame % 60) // 2
for path in (root / 'exports').glob('*.json'):
    publication = json.loads(path.read_text())
    movie = root / 'exports' / publication['movie_filename']
    assert hashlib.sha256(movie.read_bytes()).hexdigest() == publication['destination_readback']['sha256']
for name in ['docs/AUDIO_STAGE_PREPARATION.md', 'docs/RETIME_EDITING.md',
             'docs/REQUIREMENTS.md', 'docs/SECTION8_COVERAGE.md', 'docs/spec/AGENT_HANDOFF.md']:
    assert 'disk-pcm-2026-10-11.md' in (repo / name).read_text(), name
print(json.dumps({'passed': True, 'suite_tests': 1621, 'final_quota_tests': 1,
                  'old_regressions_failed': 1, 'pictures': 7200,
                  'audio_windows': summary['audio_windows_checked'],
                  'zero_offset_windows': summary['offset_verified_windows']}))
