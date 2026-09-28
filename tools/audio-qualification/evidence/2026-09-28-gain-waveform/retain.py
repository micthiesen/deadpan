"""Retain terminal waveform checks and selected, inspected GUI captures."""
import gzip
import hashlib
import json
from pathlib import Path

scratch = Path(__file__).parent
out = Path('/Users/michael/Code/deadpan/tools/audio-qualification/evidence/2026-09-28-gain-waveform')
commands = []
for path in sorted(scratch.glob('waveform*.json')):
    row = json.loads(path.read_text())
    if isinstance(row, dict) and 'command' in row and 'source_manifest_sha256' in row:
        assert 'exit_code' in row, path
        commands.append((path, row))
images = json.loads((scratch / 'images.json').read_text())
assert commands and images
out.mkdir(parents=True, exist_ok=False)

def retain(path, name, compressed=False):
    data = path.read_bytes()
    if compressed:
        data = gzip.compress(data, mtime=0)
        name += '.gz'
    target = out / name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)

sources = set()
for path, row in commands:
    retain(path, 'commands/' + path.name)
    retain(path.with_suffix('.log'), 'commands/' + path.with_suffix('.log').name, True)
    sources.add(row['source_manifest_sha256'])
for sha in sorted(sources):
    retain(scratch / ('source-' + sha + '.json'), 'sources/' + sha + '.json', True)
for path in sorted(scratch.glob('waveform*-inventory.json')):
    retain(path, 'inventories/' + path.name, True)
replays = []
for path in sorted(scratch.glob('waveform*/report.json')):
    row = json.loads(path.read_text())
    retain(path, 'replays/' + path.parent.name + '.json', True)
    replays.append({'name': path.parent.name, 'failed': row['failed'],
        'scenarios': [{'name': s['name'], 'failed': s['failed'], 'checks': len(s['checks']),
            'findings': s['findings'], 'failed_checks': [c for c in s['checks'] if not c['passed']],
            'timings': [{'name': t['name'], 'summary': t['summary']} for t in s['timings']],
            'skipped': s['skipped']} for s in row['scenarios']]})
for item in images:
    retain(scratch / item['source'], 'images/' + item['name'])
for name in ['review.md', 'check.py', 'layout_followup.py', 'feature_tests.py', 'waveform_timing.py', 'retain.py', 'audit.py', 'audit_design.py']:
    retain(scratch / name, name)
retain(Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py'), 'run.py')
(out / 'summary.json').write_text(json.dumps({'commands': [{'name': p.stem, **r} for p, r in commands],
    'replays': replays, 'images': images}, indent=2) + '\n')
(out / 'README.md').write_text('''# Measured gain waveform evidence

See [qualification](../../../../docs/qualification/gain-waveform-2026-09-28.md).
Command JSON records source identity, invocation, terminal outcome and time.
Gzip files preserve complete original logs, source inventories and replay JSON.
Source filenames hash their uncompressed inventory. Failed runs remain failed;
passing follow-ups do not rewrite those outcomes. Test populations overlap.

Only inspected selected captures are retained. Most raw replay screenshot links
are intentionally absent. No private project, media snapshot, executable or
cache is published. Waveform values in the replay come from actual qualified
PCM; the explicitly labelled failure tests inject only failure state. Comparison
delivery feedback is injected for layout checks. Those checks do not establish
acoustic, device, physical input or display-latency acceptance.

The scripts use task-specific scratch paths from the original execution. Adapt
those paths before reproducing. manifest.json hashes every file except itself.
''')
manifest = {str(p.relative_to(out)): {'bytes': p.stat().st_size,
    'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'files': len(manifest), 'bytes': sum(r['bytes'] for r in manifest.values())}))
