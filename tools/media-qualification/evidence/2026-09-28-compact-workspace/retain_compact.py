"""Retain terminal compact-workspace checks and selected inspected captures."""
import gzip
import hashlib
import json
from pathlib import Path

scratch = Path(__file__).parent
out = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-09-28-compact-workspace')
commands = []
for path in sorted(scratch.glob('compact*.json')):
    row = json.loads(path.read_text())
    if isinstance(row, dict) and 'command' in row:
        assert 'exit_code' in row, path
        commands.append((path, row))
images = json.loads((scratch / 'compact-images.json').read_text())
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
for path in sorted(scratch.glob('compact*-app-inventory.json')):
    retain(path, 'inventories/' + path.name, True)
replays = []
for path in sorted(scratch.glob('compact*/report.json')):
    row = json.loads(path.read_text())
    retain(path, 'replays/' + path.parent.name + '.json', True)
    replays.append({'name': path.parent.name, 'failed': row['failed'],
        'scenarios': [{'name': s['name'], 'failed': s['failed'], 'checks': len(s['checks']),
            'findings': s['findings'], 'failed_checks': [c for c in s['checks'] if not c['passed']],
            'timings': [{'name': t['name'], 'summary': t['summary']} for t in s['timings']],
            'skipped': s['skipped']} for s in row['scenarios']]})
for item in images:
    retain(scratch / item['source'], 'images/' + item['name'])
for name in ['compact-workspace-review.md', 'compact_followup.py', 'retain_compact.py']:
    retain(scratch / name, name)
retain(scratch / 'compact-workspace-draft/baseline-sha256.json', 'baseline-sha256.json')
(out / 'summary.json').write_text(json.dumps({'commands': [{'name': p.stem, **r} for p, r in commands],
    'replays': replays, 'images': images}, indent=2) + '\n')
(out / 'README.md').write_text('''# Compact workspace evidence

See [qualification](../../../../docs/qualification/compact-workspace-2026-09-28.md).
Command JSON records source identity, invocation, terminal outcome and time.
Gzip files preserve complete original logs, source inventories and replay JSON.
Source filenames hash their uncompressed inventory. Failed runs remain failed;
passing follow-ups do not rewrite those outcomes. Test populations overlap.

Only inspected selected captures are retained. Most raw replay screenshot links
are intentionally absent. No private project, media snapshot, executable or
cache is published. Playback feedback is injected for layout checks; it does
not establish acoustic, device, physical input or display-latency acceptance.

The commands reuse the native-gain evidence run.py, feature_tests.py and
verify_ui.py helpers. The compact continuation preserves completed app tests
and independent visual results. manifest.json hashes every file except itself.
''')
manifest = {str(p.relative_to(out)): {'bytes': p.stat().st_size,
    'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'files': len(manifest), 'bytes': sum(r['bytes'] for r in manifest.values())}))
