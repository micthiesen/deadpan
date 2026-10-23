from pathlib import Path
import subprocess,os,hashlib,json,time,shutil
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
subprocess.run(['python',str(s/'finalize_test_refinement.py')],check=True)
baseline=Path('/tmp/deadpan-first-pause-profile-2026-10-22')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
subprocess.run(['python',str(s/'freeze.py'),'release-perf'],check=True)
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
cmd=['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf'];start=time.monotonic()
with(s/'perf-build.stdout').open('w')as out,(s/'perf-build.stderr').open('w')as err:r=subprocess.run(cmd,env=env,stdout=out,stderr=err)
(s/'perf-build.json').write_text(json.dumps({'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start},indent=2)+'\n')
if r.returncode:raise SystemExit(r.returncode)
subprocess.run(['python',str(s/'guard.py'),'release-perf'],check=True)
exe=s/'candidate-perf';shutil.copy2(Path('target/release/examples/perf'),exe)
old=json.loads((baseline/'binaries.json').read_text())['baseline']
assert sha(Path(old['executed_path']))==old['sha256']
binaries={'baseline':old,'candidate':{'source_path':str(Path('target/release/examples/perf').resolve()),'executed_path':str(exe),'sha256':sha(exe)}}
(s/'binaries.json').write_text(json.dumps(binaries,indent=2)+'\n')
(s/'source.json').write_text(json.dumps({'base':'a18ff65443aa8fd1aa5ab63fa33678c9afd88220','candidate':'uncommitted Core57/SQLite86 source identities in release-perf-source-identities.json','baseline':json.loads((baseline/'source.json').read_text())},indent=2)+'\n')
print(json.dumps(binaries),flush=True)
