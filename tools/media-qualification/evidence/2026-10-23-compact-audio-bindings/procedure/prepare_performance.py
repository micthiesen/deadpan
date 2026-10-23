from pathlib import Path
import subprocess,os,hashlib,json,time,shutil
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
prior=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
subprocess.run(['python',str(s/'freeze.py'),'release-perf'],check=True)
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
cmd=['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf'];start=time.monotonic()
with(s/'perf-build.stdout').open('w')as out,(s/'perf-build.stderr').open('w')as err:r=subprocess.run(cmd,env=env,stdout=out,stderr=err)
(s/'perf-build.json').write_text(json.dumps({'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start},indent=2)+'\n')
if r.returncode:raise SystemExit(r.returncode)
subprocess.run(['python',str(s/'guard.py'),'release-perf'],check=True)
exe=s/'candidate-perf';shutil.copy2(Path('target/release/examples/perf'),exe)
old={'source_path':str(prior/'candidate-perf'),'executed_path':str(prior/'candidate-perf'),'sha256':'d8c0cd0fb54c22f8b697ed7e4c9e4f7944f6ea59b0e55f103c58d2ab61e2e7fd'}
assert sha(Path(old['executed_path']))==old['sha256']
binaries={'baseline':old,'candidate':{'source_path':str(Path('target/release/examples/perf').resolve()),'executed_path':str(exe),'sha256':sha(exe)}}
(s/'binaries.json').write_text(json.dumps(binaries,indent=2)+'\n')
(s/'source.json').write_text(json.dumps({'baseline_commit':'6916ee0d578a11c37e6e66a470eb430b02773764','candidate':'uncommitted Core58/SQLite87, exact source identities in release-perf-source-identities.json','no_p95_or_visible_hold_pass_claim':True},indent=2)+'\n')
print(json.dumps(binaries),flush=True)
