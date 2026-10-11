from pathlib import Path
import gzip
import hashlib
import json
import re

root = Path(__file__).resolve().parent
repo = root.parents[3]

def read(name):
    return json.loads((root / name).read_text())

for name, meta in read('manifest.json').items():
    data = (root / name).read_bytes()
    assert len(data) == meta['bytes'] and hashlib.sha256(data).hexdigest() == meta['sha256'], name
for path, digest in read('source-pins.json')['files'].items():
    assert hashlib.sha256((repo / path).read_bytes()).hexdigest() == digest, path
for path in root.glob('*.log.gz'):
    receipt = read(path.name.removesuffix('.log.gz') + '.json')
    assert hashlib.sha256(gzip.decompress(path.read_bytes())).hexdigest() == receipt['log_sha256'], path
for name in ['suites', 'sanitizer', 'clippy-fixed', 'clippy-final', 'final-stream-tests',
             'export-release', 'format-check']:
    assert read(name + '.json')['exit'] == 0, name
totals = [tuple(map(int, match)) for match in re.findall(
    r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',
    gzip.decompress((root / 'suites.log.gz').read_bytes()).decode(),
)]
assert all(failed == ignored == 0 for _, failed, ignored in totals)
assert sum(row[0] for row in totals) == 1629
final = gzip.decompress((root / 'final-stream-tests.log.gz').read_bytes()).decode()
assert 'shortened_streamed_output_still_admits_unused_input_dependencies ... ok' in final
assert 'two_minute_preserve_streams_under_a_budget_smaller_than_its_input ... ok' in final
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed', final))) == 14
old = gzip.decompress((root / 'baseline.log.gz').read_bytes()).decode()
assert read('baseline.json')['exit'] == 101
assert old.count('test result: FAILED. 0 passed; 1 failed;') == 2
assert 'Limit("input frames")' in old and '21.8 seconds' in old
sanitizer = read('sanitizer-report.json')
assert sanitizer['exit'] == 0
assert sanitizer['report'] == {'compared_stereo_frames': 3030017, 'maximum_input_window': 27968}
exports = read('export.json')
assert {row['fixture'] for row in exports} == {'retime-half', 'pitch-shift', 'group-saturation'}
assert all(row['passed'] and not row['failures'] for row in exports)
assert sum(row['pictures'] for row in exports) == 150
assert sum(row['audio_windows'] for row in exports) == 5
assert all(offset == ['verified_zero', 0] for row in exports for offset in row['offsets'])
for path in (root / 'exports').glob('*.json'):
    report = json.loads(path.read_text())
    movie = root / 'exports' / report['movie_filename']
    assert hashlib.sha256(movie.read_bytes()).hexdigest() == report['destination_readback']['sha256']
for name in ['docs/AUDIO_STAGE_PREPARATION.md', 'docs/RETIME_EDITING.md',
             'docs/REQUIREMENTS.md', 'docs/SECTION8_COVERAGE.md', 'docs/spec/AGENT_HANDOFF.md']:
    assert 'streamed-input-2026-10-11.md' in (repo / name).read_text(), name
print(json.dumps({'passed': True, 'suite_tests': 1629, 'focused_tests': 14,
                  'old_regressions_failed': 2, 'sanitized_stereo_frames': 3030017,
                  'pictures': 150, 'audio_windows': 5}))
