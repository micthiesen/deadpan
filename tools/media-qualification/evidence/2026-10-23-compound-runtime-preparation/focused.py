from pathlib import Path
import os,subprocess,time,json
s=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
(s/'focused-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__))},indent=2)+'\n')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_PROFILE_DEV_OPT_LEVEL='2')
rows=[]
def run(name,cmd):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:r=subprocess.run(cmd,env=env,stdout=out,stderr=err)
 rows.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'focused-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
run('format',['cargo','fmt','--all'])
run('freeze',['python',str(s/'freeze.py'),'focused'])
run('store-focused',['cargo','test','--locked','-p','deadpan-store','--test','compound_preparation'])
run('native-focused',['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--lib','project::service::macros::tests'])
run('source-guard',['python',str(s/'guard.py'),'focused'])
