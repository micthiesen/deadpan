from pathlib import Path
import gzip, hashlib, json, shutil, subprocess

scratch = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/media-qualification/evidence/2026-10-11-youtube-thumbnail'
out.mkdir(exist_ok=True)
compressed = {}

def retain(source, relative):
    target = out / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.suffix in ('.json', '.log') and source.stat().st_size > 65536:
        data = source.read_bytes()
        target = target.with_name(target.name + '.gz')
        target.write_bytes(gzip.compress(data, mtime=0))
        compressed[str(target.relative_to(out))] = {'uncompressed_bytes':len(data), 'uncompressed_sha256':hashlib.sha256(data).hexdigest()}
    else:
        shutil.copy2(source, target)

prefixes = ('app-', 'cli-', 'final-', 'followup-', 'finish-', 'milestone-', 'first-bundle-',
            'first-replay-', 'gate-source-', 'reviewed-source-', 'visual-followup-',
            'pre-anchor-', 'real-', 'network-denied-', 'proxy-failure-', 'isolated-',
            'scripted-thumbnail', 'thumbnail-replay', 'bundle')
for p in sorted(scratch.iterdir()):
    if not p.is_file():
        continue
    if (p.suffix in ('.log', '.json') and p.name.startswith(prefixes)) or (p.name.startswith('check_') and p.suffix == '.py'):
        retain(p, Path('checks') / p.name)
for folder in ('real-thumbnail', 'release-thumbnail'):
    for name in ('report.json', 'thumbnail.png'):
        retain(scratch / folder / name, Path(folder) / name)

replays = ('thumbnail-replay', 'thumbnail-replay-2', 'thumbnail-replay-3',
           'affected-replays', 'finish-youtube-replay')
for name in replays:
    root = scratch / name
    for p in sorted(root.rglob('*.json')):
        if 'bin' not in p.relative_to(root).parts:
            retain(p, Path('replays') / name / p.relative_to(root))

for name, image in [('thumbnail-replay', 'youtube-049.png'), ('thumbnail-replay-2', 'youtube-068.png')]:
    retain(scratch / name / 'youtube' / image, Path('replays') / name / 'youtube' / image)
for name, images in [('affected-replays', ['youtube-029.png', 'youtube-067.png', 'youtube-069.png', 'youtube-096.png', 'youtube-098.png', 'youtube-103.png'])]:
    for image in images:
        retain(scratch / name / 'youtube' / image, Path('replays') / name / 'youtube' / image)

final = scratch / 'finish-youtube-replay/youtube/report.json'
report = json.loads(final.read_text())
for scenario in report['scenarios']:
    for step in scenario['steps']:
        if step.get('screenshot') and any(label in step['input'] for label in (
            'Confirmation thumbnail at', 'Thumbnail confirmation', 'Long title',
            'Fresh long-title confirmation', 'optional-thumbnail status', 'Sheet confirmation')):
            image = step['screenshot']
            retain(final.parent / image, Path('replays/finish-youtube-replay/youtube') / image)

paths = subprocess.check_output(['git', 'ls-files', '-m', '-o', '--exclude-standard', '-z'], cwd=repo).decode().split('\0')
sources = {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in paths
           if p and (p.startswith('crates/') or p == 'Cargo.lock') and (repo / p).is_file()}
(out / 'source-identity.json').write_text(json.dumps({
    'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'description': 'Uncommitted final implementation over this base; file hashes bind the exact reviewed source. Verification binary hashes live in replay and real-check reports.',
    'files': sources,
}, indent=2) + '\n')

bundle = Path('/tmp/deadpan-youtube-thumbnail-final-bundle-20261011')
for name in ('Deadpan.release.json', 'Deadpan.sbom.cdx.json'):
    if (bundle / name).exists():
        retain(bundle / name, Path('bundle') / name)
executables = bundle / 'Deadpan.app/Contents/MacOS'
(out / 'bundle-binaries.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(executables.iterdir()) if p.is_file()},indent=2)+'\n')

retain(Path(__file__), Path('collect_evidence.py'))
(out / 'compression.json').write_text(json.dumps(compressed, indent=2)+'\n')
manifest = {str(p.relative_to(out)): {'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in sorted(out.rglob('*')) if p.is_file() and p.name != 'manifest.json'}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
for path, entry in manifest.items():
    assert hashlib.sha256((out/path).read_bytes()).hexdigest() == entry['sha256']
for path, entry in compressed.items():
    data = gzip.decompress((out/path).read_bytes())
    assert len(data) == entry['uncompressed_bytes']
    assert hashlib.sha256(data).hexdigest() == entry['uncompressed_sha256']
print(json.dumps({'files':len(manifest),'bytes':sum(v['bytes'] for v in manifest.values())}))
