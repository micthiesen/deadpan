from pathlib import Path
import os,subprocess,time,json,hashlib,shutil
s=Path('/tmp/deadpan-store-preparation-detail-profile-2026-10-23')
repo=Path('/Users/michael/Code/deadpan')
source=s/'source'; target=s/'target'
(s/'profile-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__))},indent=2)+'\n')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_TARGET_DIR=str(target))
rows=[]
def run(name,cmd,cwd=source):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:
  r=subprocess.run(cmd,cwd=cwd,env=env,stdout=out,stderr=err)
 rows.append({'name':name,'command':cmd,'cwd':str(cwd),'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'build-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
target.mkdir(exist_ok=False)
run('clone-release-cache',['cp','-cR',str(repo/'target/release'),str(target/'release')])
run('format',['cargo','fmt','--all'])
identities={str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest()for p in source.rglob('*.rs')}
(s/'instrumented-source-identities.json').write_text(json.dumps(identities,indent=2)+'\n')
baseline=json.loads((s/'original-source-identities.json').read_text())
changed={p for p in identities if identities[p]!=baseline.get(p)}
expected={'crates/deadpan-app/src/project/service.rs','crates/deadpan-app/src/project/service/macros.rs','crates/deadpan-store/src/compound.rs','crates/deadpan-store/src/lib.rs','crates/deadpan-store/src/prepared_compound_profile.rs'}
assert changed==expected,(changed,expected)
inputs=json.loads((s/'input-identities.json').read_text())
assert all(hashlib.sha256((source/p).read_bytes()).hexdigest()==v for p,v in inputs.items())
(s/'formatted-overlay-audit.json').write_text(json.dumps({'changed':sorted(changed),'unchanged_core_and_cli':True,'inputs_unchanged':True},indent=2)+'\n')
run('build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app'])
assert identities=={str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest()for p in source.rglob('*.rs')}
assert all(hashlib.sha256((source/p).read_bytes()).hexdigest()==v for p,v in inputs.items())
bins={}
for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
 src=target/'release'/name;dest=s/'bin'/name;dest.parent.mkdir(exist_ok=True)
 shutil.copy2(src,dest)
 bins[name]={'source':str(src),'executed':str(dest),'sha256':hashlib.sha256(dest.read_bytes()).hexdigest(),'freshly_built':name=='deadpan-app'}
(s/'binaries.json').write_text(json.dumps(bins,indent=2)+'\n')
print(json.dumps({'done':True,'app_sha256':bins['deadpan-app']['sha256']}),flush=True)
