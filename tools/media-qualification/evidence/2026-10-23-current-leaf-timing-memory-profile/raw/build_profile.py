from pathlib import Path
import os, subprocess, time, json, hashlib, shutil
s=Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23')
source=s/'source'; target=s/'target'
control=Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_TARGET_DIR=str(target))
def sha(p):
 h=hashlib.sha256()
 with p.open('rb')as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
frozen=json.loads((s/'instrumented-source-identities.json').read_text())
inputs=json.loads((s/'input-identities.json').read_text())
def guard():
 paths=subprocess.check_output(['rg','--files','-g','*.rs'],cwd=source,text=True).splitlines()
 assert {p:sha(source/p)for p in paths}==frozen
 assert {p:sha(source/p)for p in inputs}==inputs
 assert json.loads((s/'independent-review.json').read_text())['status']=='no_findings'
guard()
assert len(frozen)==2052
assert not target.exists()
target.mkdir()
(s/'profile-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__)),'started_unix':time.time(),'fresh_target':True},indent=2)+'\n')
rows=[]
def run(name,cmd):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:
  r=subprocess.run(cmd,cwd=source,env=env,stdout=out,stderr=err)
 rows.append({'name':name,'command':cmd,'cwd':str(source),'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'build-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True);guard()
 if r.returncode:raise SystemExit(r.returncode)
run('format-check',['cargo','fmt','--all','--check'])
run('build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app'])
qualified=json.loads((control/'release-native-binaries.json').read_text())
bins={}
for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
 src=target/'release'/name if name=='deadpan-app'else Path(qualified[name]['executed'])
 if name!='deadpan-app':assert sha(src)==qualified[name]['sha256']
 dest=s/'bin'/name;dest.parent.mkdir(exist_ok=True);shutil.copy2(src,dest)
 bins[name]={'source':str(src),'executed':str(dest),'sha256':sha(dest),'freshly_built':name=='deadpan-app','qualified_helper_reuse':name!='deadpan-app'}
(s/'binaries.json').write_text(json.dumps(bins,indent=2)+'\n')
guard()
print(json.dumps({'done':True,'app_sha256':bins['deadpan-app']['sha256']}),flush=True)
