"""Retain one scoped picture checkpoint, including failed invocations."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import tarfile

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/media-qualification/evidence/2026-09-29-project-pictures'
out.mkdir(parents=True, exist_ok=False)

def sha(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

def retain(path, name, compress=False):
    target = out / (name + ('.gz' if compress else ''))
    target.parent.mkdir(parents=True, exist_ok=True)
    if compress:
        with path.open('rb') as source, target.open('wb') as raw:
            with gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as encoded:
                shutil.copyfileobj(source, encoded)
    else:
        shutil.copyfile(path, target)

commands, manifests = [], set()
for path in sorted(scratch.glob('*.json')):
    record = json.loads(path.read_text())
    if not isinstance(record, dict) or 'source_manifest_sha256' not in record:
        continue
    assert 'exit_code' in record, path
    commands.append({'name': path.stem, **record})
    retain(path, 'commands/' + path.name)
    retain(path.with_suffix('.log'), 'commands/' + path.with_suffix('.log').name, True)
    manifests.add(record['source_manifest_sha256'])
for value in sorted(manifests):
    retain(scratch / ('source-' + value + '.json'), 'sources/' + value + '.json', True)

report = json.loads((scratch / 'project-metal.json').read_text())
assert report['status'] == 'passed' and all(row['passed'] for row in report['checks'])
frames = {}
for frame in report['frames']:
    path = Path(frame['path'])
    assert sha(path) == frame['sha256'] and path.stat().st_size == frame['byte_count']
    frames[path.name] = path
members = []
with (out / 'frames.tar.gz').open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as encoded:
    with tarfile.open(fileobj=encoded, mode='w|') as archive:
        for name, path in sorted(frames.items()):
            info = tarfile.TarInfo(name)
            info.size, info.mode = path.stat().st_size, 0o644
            with path.open('rb') as source:
                archive.addfile(info, source)
            members.append({'name': name, 'bytes': info.size, 'sha256': sha(path)})
(out / 'archive-contents.json').write_text(json.dumps({'frames.tar.gz': members}, indent=2) + '\n')
for name in ('project-metal.json', 'project-metal-artifact.json', 'metal-artifact.json',
             'run-native.py', 'run-metal.py', 'run-metal-initial.py', 'retain-pictures.py',
             'audit-pictures.py', 'summarize.py', 'initial-picture-tests.rs',
             'intermediate-picture-tests.rs', 'verification.json', 'review.md'):
    retain(scratch / name, name)
retain(scratch / 'evidence-readme.md', 'README.md')
(out / 'summary.json').write_text(json.dumps({'commands': commands,
    'metal': {'status': report['status'], 'checks': len(report['checks']),
              'frames': report['frame_count'], 'elapsed_seconds': report['elapsed_seconds']}}, indent=2) + '\n')
manifest = {str(path.relative_to(out)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
            for path in sorted(out.rglob('*')) if path.is_file()}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'files': len(manifest), 'bytes': sum(row['bytes'] for row in manifest.values()),
                  'frames': len(frames), 'commands': len(commands)}))
