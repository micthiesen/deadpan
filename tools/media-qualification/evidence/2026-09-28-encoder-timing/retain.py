"""Retain exact terminal evidence, including intentionally failing media cases."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import tarfile

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/media-qualification/evidence/2026-09-28-encoder-timing'
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

commands = []
inventories = set()
for path in sorted(scratch.glob('encoder*.json')):
    record = json.loads(path.read_text())
    assert 'exit_code' in record
    commands.append({'name': path.stem, **record})
    retain(path, 'commands/' + path.name)
    retain(path.with_suffix('.log'), 'commands/' + path.with_suffix('.log').name, True)
    inventories.add(record['source_manifest_sha256'])
for digest in sorted(inventories):
    retain(scratch / ('source-' + digest + '.json'), 'sources/' + digest + '.json', True)

summaries = []
media = {}
for name, path in [('matrix',scratch/'matrix.json'),('video-followup',scratch/'video-followup.json'),
                   ('sanitizers',scratch/'sanitizers.json'),('legacy',scratch/'legacy/report.json')]:
    report = json.loads(path.read_text())
    retain(path, 'reports/' + name + '.json', True)
    logs = {}
    for record in report['commands']:
        for log in record.get('logs',{}).values():
            source = Path(log['path'])
            assert source.stat().st_size == log['bytes'] and sha(source) == log['sha256']
            logs[source.name] = source
    archive(name + '-logs', logs)
    cases = []
    for case in report['cases']:
        for artifact in case.get('artifacts',{}).values():
            source = Path(artifact['path']).resolve()
            assert source.stat().st_size == artifact['bytes'] and sha(source) == artifact['sha256']
            media[str(source.relative_to(scratch))] = source
        for decoded in case.get('audio',{}).values() if 'artifacts' in case else []:
            source = Path(decoded['pcm']['path']).resolve()
            assert sha(source) == decoded['pcm']['sha256']
            media[str(source.relative_to(scratch))] = source
        cases.append({'name':case['name'],'status':case.get('status'),
                      'checks':len(case.get('checks',[])),
                      'failed_required_checks':[check['label'] for check in case.get('checks',[])
                        if not check['passed'] and not check.get('diagnostic',False)],
                      'negative_encode':case.get('known_capability_rejection',False),
                      'unqualified':case.get('unqualified',[])})
    summaries.append({'name':name,'result':report['result'],'cases':cases,
                      'commands':len(report['commands']),'process_faults':report['process_faults'],
                      'source_unchanged_during_run':report['source_unchanged_during_run'],
                      'final_file_admission':report.get('final_file_admission')})
for path in (scratch/'legacy').glob('*'):
    if path.suffix in ('.mp4','.f32'):
        media[str(path.relative_to(scratch))] = path
archive('measured-fixtures', media)

scope = ['tools/media-qualification/run.py','tools/media-qualification/media_probe.c',
         *['tools/media-qualification/compatible/' + name for name in
           ('build.py','media_probe.c','export_probe.c','encoder_oracle.py','mp4_boxes.py',
            'qualify_encoder.py','test_encoder_oracle.py','test_encoder_runner.py','test_mp4_boxes.py')]]
initial_manifest = json.loads((scratch/'source-e4b1e649bd8c9446824f9ec0a2d8091206f7d6f385bbb9d8d6b3a43a2ffd4368.json').read_text())
final_manifest = json.loads((scratch/'source-223f4fa376615e3afe42110a231a02ffa62903d614d5785e74f7b2fe6d842b74.json').read_text())
for label, base, manifest in [('initial',scratch/'initial-source',initial_manifest),('final',repo,final_manifest)]:
    for name in scope:
        assert sha(base/name) == manifest[name], name
    archive(label+'-probe-source', {name:base/name for name in scope})
retain(Path('/tmp/deadpan-ui-ffmpeg-build.json'),'build-receipt.json',True)
retain(repo/'tools/media-qualification/compatible/pins.json','pins.json')
for name in ('retain.py','audit.py','reobserve.py','legacy_smoke.py','ffmpeg-source-notes.md','review.md'):
    retain(scratch/name,name)
retain(Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py'),'run.py')
draft = Path('/tmp/deadpan-gain-waveform-4apg6qx9/export-draft')
for name in ('build-admission.md','C_PROBE_INTEGRATION.md'):
    retain(draft/name,name)
(out/'summary.json').write_text(json.dumps({'commands':commands,'reports':summaries},indent=2)+'\n')
(out/'archive-contents.json').write_text(json.dumps(archives,indent=2)+'\n')
(out/'README.md').write_text('''# SDR encoder timing evidence

See [qualification](../../../../docs/qualification/encoder-timing-2026-09-28.md).
The native and sanitizer matrices deliberately retain a nonconforming AAC path.
Their exit 1 is not a passing export gate. Video follow-up reuses original media;
the original failed run and original source bytes remain separate.

Reports and command logs are compressed without truncation. Log tar archives
contain complete original stdout/stderr. measured-fixtures.tar.gz contains all
private synthetic MP4/PCM and before-mux sidecars, including the failed hardware
B output. No user media, project, executable or library is published. Archive
member hashes/sizes are in archive-contents.json; manifest.json hashes every
retained file except itself. audit.py checks both levels without extracting.

Source inventories identify the complete checkout inputs; probe source archives
contain the relevant developer files only. The build receipt belongs to the
September 26 prefix, with fresh admission observations recorded separately.
Scripts retain their original task-specific paths; adapt paths to reproduce.
Test populations overlap. AVFoundation, closed GOP independence and full product
export are not qualified by this record.
''')
manifest = {str(path.relative_to(out)):{'bytes':path.stat().st_size,'sha256':sha(path)}
            for path in sorted(out.rglob('*')) if path.is_file()}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'files':len(manifest),'bytes':sum(row['bytes'] for row in manifest.values()),
                  'archives':len(archives),'media_members':len(media)}))
