from pathlib import Path
import os,json,subprocess,time,hashlib
root=Path(__file__).parent;repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[('format',['cargo','fmt','--all']),('clippy',['cargo','clippy','--locked','-p','deadpan-media-worker','--all-targets','--','-D','warnings']),('proxy-tests',['cargo','nextest','run','--locked','-p','deadpan-media-worker','--test','proxy_real_media','--no-fail-fast'])]
rows=[]
for name,cmd in steps:
 print('START',name,flush=True);start=time.monotonic();path=root/(name+'.log')
 with path.open('x') as log:result=subprocess.run(cmd,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
 rows.append(dict(name=name,command=cmd,exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
 (root/'checks.json').write_text(json.dumps(rows,indent=2)+'\n');print('FINISH',name,result.returncode,flush=True)
 if result.returncode:print(path.read_text()[-9000:]);raise SystemExit(result.returncode)
