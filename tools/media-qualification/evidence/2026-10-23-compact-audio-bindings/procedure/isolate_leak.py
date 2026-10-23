from pathlib import Path
import subprocess,json,os,time,hashlib
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',DEADPAN_REQUIRE_PHYSICAL_MAC='1',DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def guard():
 sources=json.loads((s/'candidate-broad-source-identities.json').read_text())
 assert set(subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines())==set(sources)
 for n,h in sources.items():assert sha(Path(n))==h,n
 for n,h in json.loads((s/'candidate-broad-inputs.json').read_text()).items():assert sha(Path(n))==h,n
guard()
rows=json.loads((s/'candidate-broad-binaries.json').read_text())
needle='owner_replacement_revokes_late_preparation_and_keeps_the_worker_bounded'
matches=[r for r in rows.values()if any(needle in t for t in r['tests'])];assert len(matches)==1
binary=matches[0];assert sha(Path(binary['path']))==binary['sha256']
cmd=['cargo','nextest','run','--archive-file',str(s/'candidate-broad.tar.zst'),'--extract-to',str(s/'candidate-broad-extracted'),'--extract-overwrite','--workspace-remap',str(Path.cwd()),'-E','test('+needle+')','--test-threads','1','--no-fail-fast']
start=time.monotonic()
with(s/'isolated-output-handle.log').open('w')as out:r=subprocess.run(cmd,env=env,stdout=out,stderr=subprocess.STDOUT)
guard();assert sha(Path(binary['path']))==binary['sha256']
(s/'isolated-output-handle-receipt.json').write_text(json.dumps({'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start,'binary':{'path':binary['path'],'sha256':binary['sha256']},'source_inputs_unchanged':True},indent=2)+'\n')
raise SystemExit(r.returncode)
