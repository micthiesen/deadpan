from pathlib import Path
import subprocess,json,time,os
s=Path('/tmp/deadpan-full-timing-closure-reuse-qualification-2026-10-23')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_PROFILE_DEV_OPT_LEVEL='2')
subprocess.run(['python',str(s/'freeze.py'),'ui-lint'],check=True)
rows=[]
for name,args in [('clippy',['cargo','clippy','--locked','-p','deadpan-app','--features','ui-harness','--all-targets','--','-D','warnings']),('format',['cargo','fmt','--all','--','--check'])]:
 start=time.monotonic()
 with(s/f'ui-lint-{name}.log').open('w')as out:r=subprocess.run(args,env=env,stdout=out,stderr=subprocess.STDOUT)
 rows.append({'name':name,'command':args,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'ui-lint-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 subprocess.run(['python',str(s/'guard.py'),'ui-lint'],check=True)
 if r.returncode:raise SystemExit(r.returncode)
