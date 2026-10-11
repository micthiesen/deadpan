from pathlib import Path
import gzip
import hashlib
import json
import subprocess

scratch = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
root = repo / 'tools/media-qualification/evidence/2026-10-11-generated-transitions'
root.mkdir(exist_ok=True)
def sha(data): return hashlib.sha256(data).hexdigest()
sources = [
    'crates/deadpan-app/src/worker.rs',
    'crates/deadpan-app/src/worker/generated_ahead.rs',
    'crates/deadpan-app/src/worker/playback_cache.rs',
    'crates/deadpan-app/src/worker/generated_playback_tests.rs',
    'crates/deadpan-app/src/preview/harness/generated_playback.rs',
    'crates/deadpan-cli/src/picture/generated.rs',
]
base = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo).decode().strip()
identity = dict(base=base, source_sha256={name: sha((repo/name).read_bytes()) for name in sources},
                baseline_app='/tmp/deadpan-generated-playback-performance-20261011/bin-verified/deadpan-app',
                baseline_app_sha256=sha(Path('/tmp/deadpan-generated-playback-performance-20261011/bin-verified/deadpan-app').read_bytes()),
                os=subprocess.check_output(['sw_vers']).decode(),
                hardware=subprocess.check_output(['sysctl', 'hw.model', 'hw.memsize', 'hw.ncpu', 'machdep.cpu.brand_string']).decode(),
                rust=subprocess.check_output(['rustc', '--version']).decode().strip(),
                fixture=str(scratch/'two-holds.deadpan'))
(scratch/'identity.json').write_text(json.dumps(identity, indent=2)+'\n')
(scratch/'source.patch').write_bytes(subprocess.check_output(['git', 'diff', '--', *sources], cwd=repo))
fixture=scratch/'two-holds.deadpan'
(scratch/'fixture-pins.json').write_text(json.dumps({str(p.relative_to(fixture)): dict(bytes=p.stat().st_size, sha256=sha(p.read_bytes())) for p in sorted(fixture.rglob('*')) if p.is_file()}, indent=2)+'\n')
manifest=[]
def add(name, data, compress=False):
    output=gzip.compress(data, mtime=0) if compress else data
    name += '.gz' if compress else ''
    target=root/name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(output)
    manifest.append(dict(path=name, bytes=len(output), sha256=sha(output), original_sha256=sha(data)))
for p in sorted(scratch.iterdir()):
    if p.is_file() and p.suffix in {'.py','.json','.stdout','.stderr','.log','.patch','.txt'}:
        add(p.name, p.read_bytes(), p.suffix in {'.stdout','.stderr','.log','.patch','.txt'})
for name in ['baseline', 'baseline-2', 'fixed-1', 'fixed-2']:
    for filename in ['report.json', 'generated-playback.json']:
        add(name+'/'+filename, (scratch/name/filename).read_bytes(), True)
for name in sources:
    add('source/'+name, (repo/name).read_bytes(), True)
    add('base/'+name, subprocess.check_output(['git','show',base+':'+name],cwd=repo), True)
(root/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(json.dumps(dict(files=len(manifest), bytes=sum(r['bytes'] for r in manifest))))
