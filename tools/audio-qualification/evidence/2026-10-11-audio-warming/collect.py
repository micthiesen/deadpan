from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess

scratch = Path('/tmp/deadpan-audio-warm-20261011')
repo = Path('/Users/michael/Code/deadpan')
root = repo/'tools/audio-qualification/evidence/2026-10-11-audio-warming'
root.mkdir(parents=True, exist_ok=True)

def save(name, value):
    (root/name).write_text(json.dumps(value, indent=2)+'\n')

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def capture(*command):
    return subprocess.check_output(command, cwd=repo, text=True).strip()

for path in scratch.iterdir():
    if path.is_file() and path.suffix in ['.json', '.py']:
        shutil.copyfile(path, root/path.name)
    elif path.is_file() and path.suffix in ['.log', '.diff']:
        (root/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))

replays = []
for run, names in [('native-replay',['audio-warming']),
                   ('native-replay-fixed',['audio-warming']),
                   ('native-replay-final',['audio-warming']),
                   ('regression-replays-final',['original-layout','sound-playback'])]:
    for name in names:
        source = scratch/run/name
        report = json.loads((source/'report.json').read_text())
        prefix = run+'-'+name
        (root/(prefix+'.json.gz')).write_bytes(gzip.compress((source/'report.json').read_bytes(), mtime=0))
        steps = [step for scenario in report['scenarios'] for step in scenario['steps']]
        selected = [step for step in steps if step['screenshot'] and
                    (any(v in step['input'] for v in ['Idle audio cache at', 'Real idle Original', 'Prepared Edit', 'Failure state'])
                     or (run=='regression-replays-final' and 'Measured sound preparation' in step['input']))]
        images = []
        for step in selected:
            destination = prefix+'-'+step['screenshot']
            shutil.copyfile(source/step['screenshot'], root/destination)
            images.append({'file': destination, 'input': step['input'], 'frame':step['frame']})
        replays.append({'run':run,'scenario':name,'failed':report['failed'],
                        'checks':sum(len(s['checks']) for s in report['scenarios']),
                        'unmeasured':[reason for s in report['scenarios'] for reason in s['skipped']],
                        'binary_sha256':report['metadata']['binary_sha256'],
                        'selected_images':images})
save('replay-summary.json',replays)
save('host.json',{'os':capture('sw_vers'), 'hardware':capture('sysctl','machdep.cpu.brand_string','hw.physicalcpu','hw.memsize'),
                 'rust':capture('rustc','-Vv'),
                 'qualified_ffmpeg_prefix':'/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'})
tracked=capture('git','diff','--name-only').splitlines()
untracked=capture('git','ls-files','--others','--exclude-standard').splitlines()
paths=['Cargo.lock','rust-toolchain.toml']+sorted(set(p for p in tracked+untracked if p.startswith('crates/')))
save('source-pins.json',{'base_revision':capture('git','rev-parse','HEAD'),'files':{p:sha(repo/p) for p in paths}})
save('manifest.json',{str(p.relative_to(root)):{'bytes':p.stat().st_size,'sha256':sha(p)}
                      for p in sorted(root.rglob('*')) if p.is_file() and p.name!='manifest.json'})
print(json.dumps({'evidence':str(root),'replays':[{k:v for k,v in r.items() if k in ['run','scenario','failed','checks']} for r in replays],
                  'bytes':sum(p.stat().st_size for p in root.iterdir() if p.is_file())}))
