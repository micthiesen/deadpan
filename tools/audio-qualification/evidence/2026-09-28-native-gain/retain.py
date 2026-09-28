"""Retain compact, lossless verification evidence after terminal checks."""
import gzip
import hashlib
import json
from pathlib import Path
import re

scratch = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-09-28-native-gain'
out.mkdir(parents=True, exist_ok=False)


def retain(source, destination=None, compressed=False):
    target = out / (destination or source.name)
    target.parent.mkdir(parents=True, exist_ok=True)
    data = source.read_bytes()
    if compressed:
        target = target.with_name(target.name + '.gz')
        data = gzip.compress(data, mtime=0)
    target.write_bytes(data)


summary = {'commands': [], 'replays': [], 'images': [], 'limits': [
    'Test populations overlap; do not sum separate invocations.',
    'Original failed runs remain failed; focused continuations are separate.',
    'Most screenshot references in raw replay reports are not retained.',
    'Private project packages, media snapshots and executables are excluded.',
    'Replay audio delivery is injected; real PCM belongs to playback tests.',
]}
base_source = json.loads((scratch / 'test-base.json').read_text())['source_manifest_sha256']
final_source = json.loads((scratch / 'final-base-app-tests.json').read_text())['source_manifest_sha256']
base_files = json.loads((scratch / ('source-' + base_source + '.json')).read_text())
final_files = json.loads((scratch / ('source-' + final_source + '.json')).read_text())
summary['source_scope'] = {'workspace_test_source': base_source, 'final_app_source': final_source,
    'changed_after_workspace_tests': [name for name in sorted(base_files.keys() | final_files.keys())
                                     if base_files.get(name) != final_files.get(name)]}
sources = set()
for path in sorted(scratch.glob('*.json')):
    report = json.loads(path.read_text())
    if not isinstance(report, dict) or 'command' not in report:
        continue
    assert 'exit_code' in report, f'Nonterminal command: {path.name}'
    retain(path, 'commands/' + path.name)
    log = path.with_suffix('.log')
    retain(log, 'commands/' + log.name, compressed=True)
    sources.add(report['source_manifest_sha256'])
    tests = [dict(zip(('passed', 'failed', 'ignored', 'measured', 'filtered'), map(int, row)))
             for row in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', log.read_text())]
    summary['commands'].append({'name': path.stem, 'exit_code': report['exit_code'],
        'seconds': report['seconds'], 'source': report['source_manifest_sha256'], 'test_targets': tests})
for source in sorted(sources):
    retain(scratch / ('source-' + source + '.json'), 'sources/' + source + '.json', compressed=True)
for path in sorted(scratch.glob('*-app-inventory.json')):
    retain(path, 'inventories/' + path.name, compressed=True)
for path in sorted(scratch.glob('*/report.json')):
    report = json.loads(path.read_text())
    retain(path, 'replays/' + path.parent.name + '.json', compressed=True)
    summary['replays'].append({'name': path.parent.name, 'failed': report['failed'],
        'mode': report['mode'], 'scenarios': [{'name': row['name'], 'failed': row['failed'],
            'checks': len(row['checks']), 'failed_checks': [c for c in row['checks'] if not c['passed']],
            'timings': [{'name': timing['name'], 'summary': timing['summary']}
                        for timing in row['timings']],
            'skipped': row['skipped']} for row in report['scenarios']]})
images = [
    ('gain-visual-03/gain-121.png', 'failed-minimum-picture.png'),
    ('gain-visual-04/gain-121.png', 'failed-curve-menu.png'),
    ('gain-visual-05/gain-124.png', 'failed-tab-boundary.png'),
    ('gain-visual-08/gain-122.png', 'failed-focused-field.png'),
    ('full-ui-visual/workspace-056.png', 'failed-resize-containment.png'),
    ('full-ui-visual/room-tone-090.png', 'failed-hold-inspector.png'),
    ('corrected-visual-gain/gain-121.png', 'gain-minimum.png'),
    ('corrected-visual-gain/gain-122.png', 'gain-default.png'),
    ('corrected-visual-room-tone/room-tone-090.png', 'hold-inspector-minimum.png'),
    ('corrected-visual-room-tone/room-tone-098.png', 'hold-inspector-default.png'),
]
for original, name in images:
    retain(scratch / original, 'images/' + name)
    summary['images'].append({'source': original, 'retained': 'images/' + name})
for name in ['review.md', 'gain-final-evidence-review.md', 'feature_tests.py', 'verify_ui.py', 'final_checks.py', 'prepare_native.py', 'retain.py']:
    retain(scratch / name)
retain(Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py'), 'run.py')
native = scratch / 'native'
for path in sorted(native.glob('*.json')):
    retain(path, 'native/' + path.name, compressed=path.name.startswith('project-'))
for path in sorted(native.glob('*.png')):
    retain(path, 'native/' + path.name)
if (native / 'review.md').exists():
    retain(native / 'review.md', 'native/review.md')
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
(out / 'README.md').write_text('''# Native gain evidence

See the [qualification record](../../../../docs/qualification/native-gain-2026-09-28.md)
for implementation, review corrections, verified boundaries and remaining work.

Command JSON records invocations, source manifests, start times, terminal exits
and elapsed time. Matching gzip logs preserve complete output. Source inventory
filenames hash the uncompressed JSON. Documentation/evidence updates after a run
are not claimed as compiled source. Inventories identify exact Cargo artifacts.

`summary.json` derives command test counts and replay assertions without adding
overlapping populations. Failed runs retain their original outcome; passing
focused follow-ups do not rewrite those outcomes. Durations including compilation
are not product latency measurements. Replay timings describe their own workloads.

Compressed replay JSON retains all diagnostics but most referenced PNGs are
intentionally omitted. Selected actual failures and final default/minimum gain
captures remain under `images/`. Native evidence, when present, is separate.
No private project, media snapshot, executable or cache is published here.

Gain replay uses production writer/media/GPU paths and injected typed audio
delivery. Real qualified PCM is covered separately by playback tests. Physical
keyboard layouts, OS IME, VoiceOver, listening, long-media response and encoded
export are not established by these reports.

`manifest.json` hashes every retained file except itself.
''')
manifest = {str(path.relative_to(out)): {'bytes': path.stat().st_size,
    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
    for path in sorted(out.rglob('*')) if path.is_file()}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'directory': str(out), 'files': len(manifest),
                  'bytes': sum(row['bytes'] for row in manifest.values())}))
