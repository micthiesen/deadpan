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
for name in ['suites-complete', 'residency-final', 'clippy', 'export-release', 'format-check']:
    assert read(name + '.json')['exit'] == 0, name
totals = [tuple(map(int, match)) for match in re.findall(
    r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',
    gzip.decompress((root / 'suites-complete.log.gz').read_bytes()).decode(),
)]
assert all(failed == ignored == 0 for _, failed, ignored in totals)
assert sum(row[0] for row in totals) == 961
old = gzip.decompress((root / 'baseline.log.gz').read_bytes()).decode()
assert read('baseline.json')['exit'] == 101
assert 'test result: FAILED. 0 passed; 4 failed;' in old
report = read('export-report.json')
assert report['maximum_oracle_error'] < 0.000001 and report['oracle_samples'] == 256
export = report['report']
assert export['passed'] and not export['failures']
assert export['summary']['pictures_checked'] == 54
assert export['summary']['audio_windows_checked'] == 2
assert all(window['offset_status'] == 'verified_zero' and window['measured_offset_samples'] == 0
           and not window['flags'] for window in export['audio'])
assert hashlib.sha256((root / 'group-saturation.mp4').read_bytes()).hexdigest() == read('artifact-pins.json')['movie_sha256']
for name in ['docs/AUDIO_GAIN.md', 'docs/REQUIREMENTS.md', 'docs/SECTION8_COVERAGE.md', 'docs/spec/AGENT_HANDOFF.md']:
    assert 'group-clocks-2026-10-11.md' in (repo / name).read_text(), name
print(json.dumps({'passed': True, 'tests': sum(row[0] for row in totals),
                  'old_regressions_failed': 4, 'pictures': 54, 'audio_windows': 2}))
