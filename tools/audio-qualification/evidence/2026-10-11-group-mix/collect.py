from pathlib import Path
import gzip, hashlib, json, shutil, tarfile
scratch = Path('/tmp/deadpan-group-mix-20261011')
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-10-11-group-mix'
out.mkdir(parents=True, exist_ok=True)
for path in scratch.glob('*.log'):
    (out / (path.name + '.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))
for path in scratch.glob('*.json'):
    if path.name in {'source-pins.json', 'export.json'}:
        continue
    name = 'source-pins.json' if path.name == 'source-pins-final.json' else path.name
    shutil.copyfile(path, out / name)
for name in ['run.py', 'check.py', 'collect.py']:
    shutil.copyfile(scratch / name, out / name)
export = json.loads((scratch / 'export.json').read_text())
shutil.copyfile(scratch / 'export.json', out / 'export-report.json')
shutil.copyfile(export['movie'], out / 'group-saturation.mp4')
with tarfile.open(out / 'fixture.tar.gz', 'w:gz') as archive:
    archive.add(Path(export['package']).parent, arcname='group-saturation')
pins = {'movie_sha256': hashlib.sha256((out / 'group-saturation.mp4').read_bytes()).hexdigest(),
    'deadpan_cli_sha256': hashlib.sha256((repo / 'target/release/deadpan-cli').read_bytes()).hexdigest(),
    'preview_export_test_sha256': hashlib.sha256((repo / 'target/release/deps/preview_export-748171a0b2c2a124').read_bytes()).hexdigest()}
(out / 'artifact-pins.json').write_text(json.dumps(pins, indent=2) + '\n')
manifest = {path.name: {'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
    for path in sorted(out.iterdir()) if path.is_file() and path.name != 'manifest.json'}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'files': len(manifest), 'bytes': sum(value['bytes'] for value in manifest.values()), 'pins': pins}))
