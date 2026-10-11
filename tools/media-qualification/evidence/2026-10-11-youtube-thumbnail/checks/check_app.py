from pathlib import Path
import os, subprocess, time, json, hashlib
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[('app-fmt',['cargo','fmt','--all']),('app-clippy',['cargo','clippy','--locked','-p','deadpan-app','--all-targets','--features','ui-harness','--','-D','warnings']),('app-youtube-tests',['cargo','nextest','run','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app','-E','test(youtube)']),('thumbnail-replay',['cargo','xtask','replays','--jobs','1','--scenario','youtube','--output',str(root/'thumbnail-replay')])]
rows=[]
for name,cmd in steps:
 print('START',name,flush=True)
 start=time.monotonic(); log=root/(name+'.log')
 with log.open('x') as out:
  result=subprocess.run(cmd,cwd=repo,env=env,stdout=out,stderr=subprocess.STDOUT)
 rows.append(dict(name=name,command=cmd,exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
 (root/'app-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,flush=True)
 if result.returncode:
  print(log.read_text(errors='replace')[-12000:],flush=True)
  raise SystemExit(result.returncode)
