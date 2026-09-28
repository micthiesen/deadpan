"""Retain synthetic SDR observations, complete logs and exact source identities."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import tarfile

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/media-qualification/evidence/2026-09-28-sdr-pixels'
out.mkdir(parents=True, exist_ok=False)
archives = {}

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def retain(path, name, compressed=False):
    target = out / (name + ('.gz' if compressed else ''))
    target.parent.mkdir(parents=True, exist_ok=True)
    if compressed:
        with path.open('rb') as source, target.open('wb') as raw:
            with gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as encoded:
                shutil.copyfileobj(source, encoded)
    else:
        shutil.copyfile(path, target)

def archive(name, paths):
    records = []
    target = out / (name + '.tar.gz')
    with target.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode='w|') as output:
            for member, path in sorted(paths.items()):
                expected = sha(path)
                info = tarfile.TarInfo(member)
                info.size, info.mode = path.stat().st_size, 0o644
                with path.open('rb') as source:
                    output.addfile(info, source)
                assert sha(path) == expected, path
                records.append({'name': member, 'bytes': info.size, 'sha256': expected})
    archives[target.name] = records

commands, manifests = [], set()
for path in sorted(scratch.glob('sdr[0-9][0-9]-*.json')):
    record = json.loads(path.read_text())
    assert 'exit_code' in record
    commands.append({'name': path.stem, **record})
    retain(path, 'commands/' + path.name)
    retain(path.with_suffix('.log'), 'commands/' + path.with_suffix('.log').name, True)
    manifests.add(record['source_manifest_sha256'])
for value in sorted(manifests):
    retain(scratch / ('source-' + value + '.json'), 'sources/' + value + '.json', True)

media, reports = {}, []
metal = json.loads((scratch/'metal.json').read_text())
retain(scratch/'metal.json', 'reports/metal.json', True)
retain(scratch/'metal-artifact.json', 'metal-artifact.json')
for case in metal['cases']:
    for key, hash_key in (('raw_path', 'actual_sha256'), ('reference_path', 'reference_sha256')):
        path = Path(case[key])
        assert sha(path) == case[hash_key] and path.stat().st_size == case['byte_count']
        media[str(path.relative_to(scratch))] = path

for path in [*sorted(scratch.glob('picture*.json')), scratch/'encoder-control.json']:
    report = json.loads(path.read_text())
    if 'commands' not in report or 'cases' not in report:
        continue
    retain(path, 'reports/' + path.name, True)
    logs = {}
    for command in report['commands']:
        for item in command.get('logs', {}).values():
            source = Path(item['path'])
            assert sha(source) == item['sha256'] and source.stat().st_size == item['bytes']
            logs[source.name] = source
    archive(path.stem + '-logs', logs)
    for case in report['cases']:
        items = list(case.get('artifacts', {}).values())
        items.extend(value['pcm'] for value in case.get('audio', {}).values())
        for item in items:
            source = Path(item['path'])
            assert sha(source) == item['sha256'] and source.stat().st_size == item['bytes']
            media[str(source.relative_to(scratch))] = source
    reports.append({'name': path.stem, 'result': report['result'],
                    'experiment_completed': report['experiment_completed'],
                    'process_faults': report['process_faults'],
                    'source_unchanged_during_run': report['source_unchanged_during_run'],
                    'final_file_admission': {'checks': len(report['final_file_admission']),
                                             'all_passed': all(row['passed'] for row in report['final_file_admission'])},
                    'cases': [{key: case[key] for key in ('name', 'status', 'checks', 'decoded_vs_actual') if key in case}
                              for case in report['cases']]})
archive('measured-fixtures', media)
initial = json.loads((scratch/'picture.json').read_text())['source_sha256_at_start']['compatible/export_probe.c']
assert sha(scratch/'initial-export-probe.c') == initial
retain(scratch/'initial-export-probe.c', 'initial-export-probe.c')
for name in ('run-native.py', 'run-metal.py', 'check-encoder-control.py', 'summarize.py', 'retain-sdr.py', 'audit-sdr.py', 'review.md', 'verification.json'):
    retain(scratch/name, name)
(out/'summary.json').write_text(json.dumps({'commands': commands, 'metal': metal, 'reports': reports}, indent=2)+'\n')
(out/'archive-contents.json').write_text(json.dumps(archives, indent=2)+'\n')
retain(scratch/'evidence-readme.md', 'README.md')
manifest = {str(path.relative_to(out)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
            for path in sorted(out.rglob('*')) if path.is_file()}
(out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(json.dumps({'files': len(manifest), 'bytes': sum(v['bytes'] for v in manifest.values()),
                  'archives': len(archives), 'media_members': len(media)}))
