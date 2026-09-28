"""Retain the independent reader comparison, including the initial schema failure."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import tarfile

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/media-qualification/evidence/2026-09-28-native-audio'
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
                info.size = path.stat().st_size
                info.mode = 0o644
                with path.open('rb') as source:
                    output.addfile(info, source)
                assert sha(path) == expected, path
                records.append({'name': member, 'bytes': info.size, 'sha256': expected})
    archives[target.name] = records

commands, manifests = [], set()
for path in sorted(scratch.glob('native0*.json')):
    record = json.loads(path.read_text())
    assert 'exit_code' in record
    commands.append({'name': path.stem, **record})
    retain(path, 'commands/' + path.name)
    retain(path.with_suffix('.log'), 'commands/' + path.with_suffix('.log').name, True)
    manifests.add(record['source_manifest_sha256'])
for value in sorted(manifests):
    retain(scratch / ('source-' + value + '.json'), 'sources/' + value + '.json', True)

summaries, media = [], {}
for name in ('native', 'native-bools', 'native-sanitizers'):
    path = scratch / (name + '.json')
    report = json.loads(path.read_text())
    retain(path, 'reports/' + path.name, True)
    logs = {}
    for command in report['commands']:
        for item in command.get('logs', {}).values():
            source = Path(item['path'])
            assert sha(source) == item['sha256'] and source.stat().st_size == item['bytes']
            logs[source.name] = source
    archive(name + '-logs', logs)
    cases = []
    for case in report['cases']:
        for key in ('input', 'pcm'):
            item = case[key]
            source = Path(item['path']).resolve()
            assert sha(source) == item['sha256'] and source.stat().st_size == item['bytes']
            media[str(source.relative_to(scratch.resolve()))] = source
        oracle = case['oracle']
        raw = oracle['observations']['raw_native_observation']
        cases.append({'name': case['name'], 'status': case['status'],
            'probe_exit_code': case['probe_exit_code'], 'checks': len(oracle['checks']),
            'failed_required_checks': [v for v in oracle['checks'] if not v['passed'] and not v.get('diagnostic')],
            'measurement': oracle['observations']['native'], 'unqualified': oracle['unqualified'],
            'track': raw['track'], 'pcm_samples': raw['pcm']['total_samples'],
            'stored_buffers': raw['stored']['buffer_count'], 'pcm_buffers': raw['pcm']['buffer_count']})
    summaries.append({'name': name, 'result': report['result'], 'commands': len(report['commands']),
        'experiment_completed': report['experiment_completed'], 'cases': cases,
        'process_faults': report['process_faults'],
        'source_unchanged_during_run': report['source_unchanged_during_run'],
        'final_file_admission': report['final_file_admission']})
archive('measured-fixtures', media)
retain(scratch/'native-encoder-regression.json', 'reports/encoder-regression.json', True)
initial = json.loads((scratch/'native.json').read_text())['source_sha256_at_start']
assert sha(scratch/'native-initial-probe.m') == initial['avfoundation_probe.m']
retain(scratch/'native-initial-probe.m', 'initial-avfoundation-probe.m')
final = json.loads((scratch/'native-sanitizers.json').read_text())['source_sha256']
scope = {name: repo/'tools/media-qualification/compatible'/name for name in final}
for name, source in scope.items():
    assert sha(source) == final[name], name
archive('qualified-native-source', scope)
for name in ('run-native.py', 'reevaluate-encoder.py', 'retain-native.py', 'audit-native.py', 'native-review.md'):
    retain(scratch/name, name)
(out/'summary.json').write_text(json.dumps({'commands': commands, 'reports': summaries}, indent=2)+'\n')
(out/'archive-contents.json').write_text(json.dumps(archives, indent=2)+'\n')
(out/'README.md').write_text('''# Independent AVFoundation audio evidence

See [qualification](../../../../docs/qualification/native-audio-2026-09-28.md).
Both readers complete. The default-edit-list file aligns; the disabled file
loses its opening impulse and shifts subsequent events 1,088 samples early.
The nonzero exit is a retained timing failure, not a passing export gate.

Initial output boxed CoreMedia Boolean values as numeric JSON. The strict
oracle rejected that schema; the original source, logs and raw PCM remain here.
The corrected and sanitizer observations explicitly use JSON booleans.

Full command logs, raw observations, synthetic input MP4s and untouched output
PCM are compressed without truncation. No user media, executable, library or
project is published. The input files are byte-identical to the preceding
encoder experiment. No event alignment, manual trim or extra encode occurred.
Source inventories record concurrent checkout state; the native reports check
their own complete local source dependencies, which stayed unchanged per run.
The Rust renderer changes in the checkout were outside these Python/native tests.

manifest.json hashes every retained file except itself. archive-contents.json
records every archive member. Run audit-native.py to verify both levels without
extracting files. Scripts retain their task-specific paths for attribution.
The raw reports distinguish completed reading, timing failure and unqualified
interpretations. These results do not qualify physical playback, native video,
boundary-content quality, other macOS versions or product export.
''')
manifest = {str(path.relative_to(out)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
            for path in sorted(out.rglob('*')) if path.is_file()}
(out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(json.dumps({'files': len(manifest), 'bytes': sum(v['bytes'] for v in manifest.values()),
                  'archives': len(archives), 'media_members': len(media)}))
