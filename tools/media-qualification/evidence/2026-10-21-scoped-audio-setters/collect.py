"""Retain bounded evidence only after root authorizes final native reports.

Run from the repository root: python /tmp/.../collect.py --confirmed-final
Root must first write inspected-captures.json as a list of objects containing
path (absolute scratch PNG), checkpoint, sha256 and inspected_by="root".
Include one inspected failure capture from each failed run and 1-4 final PNGs.
"""
from pathlib import Path
import gzip
import hashlib
import json
import sys

SCRATCH = Path('/tmp/deadpan-scoped-audio-setters-2026-10-21')
OUTPUT = Path('tools/media-qualification/evidence/2026-10-21-scoped-audio-setters')
PINS = {
    'cli': ('target/release/deadpan-cli', 'aa877e6402db34495f53033191edfb51fee9f783875a5d70c9a9bf9f46888c89'),
    'test': ('target/release/deps/preview_export-8652fc5e7c449500', 'd30137f0d1af9ddc3b25de676a59c04b71f86cfdd0ae2c60038e974019668014'),
    'original': ('native/deadpan-source/tests/fixtures/cfr-bframes.mp4', '5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918'),
}

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def read(name):
    return json.loads((SCRATCH / name).read_text())

assert sys.argv[1:] == ['--confirmed-final'], 'Root authorization and final reports required'
sources = read('source-identities.json')
for name, expected in sources.items():
    assert sha(name) == expected, ('source changed', name)
assert any(name.endswith('scoped_plays/pause_macros.rs') for name in sources)
assert any(name.endswith('scoped_plays/pauses.rs') for name in sources)
for name, (path, expected) in PINS.items():
    assert sha(path) == expected, ('executed input identity', name, path)

first = read('first-visual/report.json')
assert first['failed'] is True
first_scenario = next(s for s in first['scenarios'] if s['name'] == 'scoped-plays')
failed_checks = [check['name'] for check in first_scenario['checks'] if not check['passed']]
assert failed_checks == ['A delayed pause cannot reclaim another play or append into an abandoned recording']
second = read('second-visual/report.json')
assert second['failed'] is True
second_scenario = next(s for s in second['scenarios'] if s['name'] == 'scoped-plays')
assert all(check['passed'] for check in second_scenario['checks'])
second_failures = [finding['message'] for finding in second_scenario.get('findings', [])
                   if finding['severity'] == 'failure']
assert len(second_failures) == 1 and second_failures[0].startswith(
    'Expected one visible enabled control "Your edit  :sequence", found 0.')

reports = {mode: read(f'{mode}/report.json') for mode in ['visual', 'performance']}
for mode, report in reports.items():
    assert report['failed'] is False, mode
    assert all(check['passed'] for s in report['scenarios'] for check in s['checks']), mode
for name, item in read('binaries.json').items():
    assert sha(item['executed']) == item['sha256'], ('final replay binary', name)
for attempt in ['first', 'second']:
    for name, item in read(f'{attempt}-binaries.json').items():
        assert sha(SCRATCH / f'{attempt}-bin' / name) == item['sha256'], (attempt, 'replay binary', name)

encoded = read('encoded-result.json')
report = encoded['report']
assert report['passed'] is True and report['failures'] == []
assert report['summary']['pictures_checked'] == 72 and report['summary']['pictures_failed'] == 0
assert len(report['pictures']) == 72
assert report['movie']['audio']['presented_end'] == 115315
assert report['movie']['audio_edits']['presented_media_ticks'] == 115315
assert report['movie']['expected_audio_samples'] == 115315
assert report['summary']['nonzero_offsets'] == 0
assert report['summary']['signal_windows'] == 6 and report['summary']['audio_windows_checked'] == 6
assert len(report['audio']) == 6
end = 0
for window in report['audio']:
    assert window['window'][0] == end
    end = window['window'][1]
    assert window['project_samples'] == window['window']
    assert window['passed'] is True and window['flags'] == []
    assert window['offset_status'] == 'verified_zero' and window['measured_offset_samples'] == 0
    assert window['blocks']['low_snr_blocks'] == 0
assert end == 115315
package = Path(encoded['package'])
oracle = package.parent / 'planned-pcm-oracle.json'
movie = Path(encoded['movie'])
retained_original = package.parent / 'original.mp4'
assert sha(retained_original) == PINS['original'][1]
assert len(json.loads(oracle.read_text())['occurrences']) == 6

captures = read('inspected-captures.json')
assert 3 <= len(captures) <= 6, 'Retain two failed captures and 1-4 useful final PNGs'
capture_runs = []
for capture in captures:
    path = Path(capture['path'])
    assert path.parent in [SCRATCH / name for name in ['first-visual', 'second-visual', 'visual']]
    assert path.suffix == '.png'
    assert capture['inspected_by'] == 'root' and capture['checkpoint']
    assert sha(path) == capture['sha256']
    capture_runs.append(path.parent.name)
assert capture_runs.count('first-visual') == 1 and capture_runs.count('second-visual') == 1
assert 1 <= capture_runs.count('visual') <= 4

OUTPUT.mkdir(exist_ok=True)
def keep(source, name=None, compress=False):
    source = Path(source)
    content = source.read_bytes()
    name = name or source.name
    if compress:
        content = gzip.compress(content, mtime=0)
        name += '.gz'
    (OUTPUT / name).write_bytes(content)

for path in sorted(SCRATCH.iterdir()):
    if path.is_file() and path.suffix in ['.log', '.json', '.sh', '.txt', '.diff', '.exit', '.py', '.sha256', '.patch']:
        keep(path, compress=path.suffix in ['.log', '.diff'] or path.stat().st_size > 65536)
keep(SCRATCH / 'old-witness/scoped_audio_setters.rs', 'old-witness-scoped-audio-setters.rs', True)
for name in sources:
    if name.endswith('.rs'):
        keep(name, name.replace('/', '__'), True)
keep(oracle, 'planned-pcm-oracle.json')
keep(SCRATCH / 'first-visual/report.json', 'first-visual-report.json', True)
keep(SCRATCH / 'second-visual/report.json', 'second-visual-report.json', True)

summary = {}
for mode, native in reports.items():
    keep(SCRATCH / mode / 'report.json', f'{mode}-report.json', True)
    scenario = next(s for s in native['scenarios'] if s['name'] == 'scoped-plays')
    summary[mode] = {
        'failed': native['failed'], 'checks': len(scenario['checks']),
        'findings': [{'severity': finding['severity'], 'message_prefix': finding['message'][:240]}
                     for finding in scenario.get('findings', [])],
        'timings': {timing['name']: timing['summary'] for timing in scenario['timings']
                    if timing['name'] in ['input_to_state_ms', 'input_to_picture_complete_ms']},
    }
for capture in captures:
    source = Path(capture['path'])
    name = source.parent.name + '-' + source.name
    keep(capture['path'], name)
    capture['file'] = name
(OUTPUT / 'captures.json').write_text(json.dumps(captures, indent=2) + '\n')
(OUTPUT / 'replay-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
provenance = {
    'executed_encoded_binaries': {name: {'path': path, 'sha256': expected}
                                 for name, (path, expected) in PINS.items() if name != 'original'},
    'original': {'path': PINS['original'][0], 'retained_copy': str(retained_original),
                 'sha256': sha(retained_original), 'bytes': retained_original.stat().st_size},
    'movie': {'path': str(movie), 'sha256': sha(movie), 'bytes': movie.stat().st_size},
    'project': {'path': str(package), 'project_id': report['project_id'],
                'revision_id': report['revision_id'], 'document_sha256': report['document_sha256']},
    'policy': 'Reports, PCM oracle and media identities retained; raw Original, movie, package and executables omitted.',
}
(OUTPUT / 'media-provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
manifest = {path.name: {'bytes': path.stat().st_size, 'sha256': sha(path)}
            for path in sorted(OUTPUT.iterdir()) if path.is_file() and path.name != 'inventory.json'}
total = sum(item['bytes'] for item in manifest.values())
assert total < 8 * 1024 * 1024, ('Unexpectedly large evidence collection', total)
(OUTPUT / 'inventory.json').write_text(json.dumps(manifest, indent=2) + '\n')
for name, item in manifest.items():
    assert sha(OUTPUT / name) == item['sha256'], name
print(f'{len(manifest)} evidence files, {total} bytes; inventory verified')
