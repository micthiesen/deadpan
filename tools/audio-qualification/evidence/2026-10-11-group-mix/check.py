from pathlib import Path
import gzip, hashlib, json, re
root = Path(__file__).resolve().parent
repo = root.parents[3]
read = lambda name: json.loads((root / name).read_text())
for name, meta in read('manifest.json').items():
    data = (root / name).read_bytes()
    assert len(data) == meta['bytes'] and hashlib.sha256(data).hexdigest() == meta['sha256'], name
pins = read('source-pins.json')
for path, digest in pins['files'].items():
    assert hashlib.sha256((repo / path).read_bytes()).hexdigest() == digest, path
for name in ['audio-check', 'audio-suite', 'final-audio-playback', 'retained-clock-test', 'clippy', 'audio-clippy-final', 'export-release', 'format-check']:
    result = read(name + '.json')
    assert result['exit'] == 0, name
    log = gzip.decompress((root / (name + '.log.gz')).read_bytes())
    assert hashlib.sha256(log).hexdigest() == result['log_sha256'], name
full = gzip.decompress((root / 'final-audio-playback.log.gz').read_bytes()).decode()
audio, playback = full.split('Running unittests src/lib.rs (target/debug/deps/deadpan_playback')
for name, log, count in [('audio', audio, 485), ('playback', playback, 72)]:
    totals = [tuple(map(int, x)) for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)]
    assert tuple(sum(row[i] for row in totals) for i in range(3)) == (count, 0, 0), name
assert '1 passed; 0 failed' in gzip.decompress((root / 'retained-clock-test.log.gz').read_bytes()).decode()
old = gzip.decompress((root / 'old-regression.log.gz').read_bytes()).decode()
assert read('old-regression.json')['exit'] == 101
assert 'test result: FAILED. 2 passed; 2 failed;' in old
assert all(name in old for name in read('old-witness.json')['expected_failures'])
export = read('export-report.json')
assert export['maximum_oracle_error'] < 0.000001 and export['oracle_samples'] == 256
report = export['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked'] == 54
assert report['summary']['audio_windows_checked'] == 2
assert all(window['offset_status'] == 'verified_zero' and window['measured_offset_samples'] == 0 and not window['flags'] for window in report['audio'])
assert hashlib.sha256((root / 'group-saturation.mp4').read_bytes()).hexdigest() == read('artifact-pins.json')['movie_sha256']
for name in ['docs/AUDIO_GAIN.md', 'docs/REQUIREMENTS.md', 'docs/SECTION8_COVERAGE.md']:
    assert 'group-mix-2026-10-11.md' in (repo / name).read_text(), name
print(json.dumps({'passed': True, 'audio_tests': 486, 'playback_tests': 72, 'old_regressions_failed': 2,
    'pictures': 54, 'audio_windows': 2, 'oracle_samples': 256, 'source_pins': len(pins['files'])}))
