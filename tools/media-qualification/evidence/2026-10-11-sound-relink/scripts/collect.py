from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess

repo = Path('/Users/michael/Code/deadpan')
root = Path(__file__).parent
out = repo/'tools/media-qualification/evidence/2026-10-11-sound-relink'
out.mkdir(parents=True, exist_ok=True)
compression = []
def retain(source, relative):
    data = source.read_bytes()
    target = out/relative
    if len(data) > 32_768 and source.suffix in ['.log', '.json']:
        target = target.with_suffix(target.suffix+'.gz')
        compression.append({'path': str(target.relative_to(out)), 'original_bytes': len(data),
                            'original_sha256': hashlib.sha256(data).hexdigest()})
        data = gzip.compress(data, mtime=0)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)

for source in root.iterdir():
    if source.is_file() and source.suffix in ['.log', '.json', '.py']:
        retain(source, Path('scripts' if source.suffix == '.py' else 'checks')/source.name)
for folder in ['replays', 'replays-final', 'replays-final-2', 'replays-final-3']:
    if not (root/folder/'summary.json').exists():
        continue
    retain(root/folder/'summary.json', Path(folder)/'summary.json')
    for source in (root/folder).iterdir():
        if source.is_file() and source.suffix == '.log':
            retain(source, Path(folder)/source.name)
        if source.is_dir() and (source/'report.json').exists():
            retain(source/'report.json', Path(folder)/source.name/'report.json')
            if folder == 'replays-final-3' and source.name == 'sound-relink':
                report = json.loads((source/'report.json').read_text())
                for scenario in report['scenarios']:
                    for step in scenario['steps']:
                        if step.get('semantic', {}).get('checkpoint') in ['Selected sound repaired', 'Missing sound report']:
                            retain(source/step['screenshot'], Path(folder)/source.name/step['screenshot'])
files = set(subprocess.check_output(['git','diff','--name-only','--','crates/deadpan-app'], cwd=repo, text=True).splitlines())
files.add('crates/deadpan-app/src/preview/harness/sound_relink.rs')
files.update(['Cargo.lock', 'rust-toolchain.toml'])
files.update(['native/deadpan-source/tests/fixtures/cfr-bframes.mp4',
              'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav',
              'native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav'])
identity = {'base_revision': subprocess.check_output(['git','rev-parse','HEAD'], cwd=repo, text=True).strip(),
            'files': {name: hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in sorted(files)},
            'note': 'Final source hashes. Initial compile/reference rewrite and replay failures are retained separately; replay reports identify each executed binary.'}
(out/'source-identity.json').write_text(json.dumps(identity, indent=2)+'\n')
(out/'compression.json').write_text(json.dumps(compression, indent=2)+'\n')
manifest = []
for source in sorted(out.rglob('*')):
    if source.is_file() and source.name != 'SHA256SUMS':
        manifest.append(f'{hashlib.sha256(source.read_bytes()).hexdigest()}  {source.relative_to(out)}')
(out/'SHA256SUMS').write_text('\n'.join(manifest)+'\n')
print(f'Retained {len(manifest)} evidence files in {out}')
