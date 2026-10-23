from pathlib import Path
import os,subprocess,time,json,hashlib,shutil
s=Path('/tmp/deadpan-compact-json-size-2026-10-23');repo=Path('/Users/michael/Code/deadpan')
source=s/'source';target=s/'target'
(s/'build-resumed-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__))},indent=2)+'\n')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_TARGET_DIR=str(target),CARGO_PROFILE_DEV_OPT_LEVEL='2')
rows=[]
def run(name,cmd,expected=None):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:r=subprocess.run(cmd,cwd=source,env=env,stdout=out,stderr=err)
 rows.append({'name':name,'command':cmd,'cwd':str(source),'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'build-resumed-receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
 if expected:assert expected in(s/(name+'.stdout')).read_text(),name
assert target.is_dir() and (target/'debug').is_dir()
run('clone-release-cache',['cp','-cR',str(repo/'target/release'),str(target/'release')])
run('format',['cargo','fmt','--all'])
def identities():return {str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest()for p in source.rglob('*.rs')}
frozen=identities();(s/'build-source-identities.json').write_text(json.dumps(frozen,indent=2)+'\n')
run('core-count-tests',['cargo','test','--locked','-p','deadpan-core','--lib','document::serialization_tests::compact_'],'2 passed; 0 failed')
assert identities()==frozen
run('binding-tests',['cargo','test','--locked','-p','deadpan-core','--test','compact_audio_bindings'])
assert identities()==frozen
run('store-checkpoint-test',['cargo','test','--locked','-p','deadpan-store','--lib','compact_size_counting_retains_exact_staged_checkpoint_bytes_and_history'],'1 passed; 0 failed')
assert identities()==frozen
run('release-build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app'])
assert identities()==frozen
bins={}
for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
 src=target/'release'/name;dest=s/'bin'/name;dest.parent.mkdir(exist_ok=True);shutil.copy2(src,dest)
 bins[name]={'source':str(src),'executed':str(dest),'sha256':hashlib.sha256(dest.read_bytes()).hexdigest(),'freshly_built':name=='deadpan-app'}
(s/'binaries.json').write_text(json.dumps(bins,indent=2)+'\n')
print(json.dumps({'done':True,'app_sha256':bins['deadpan-app']['sha256']}),flush=True)
