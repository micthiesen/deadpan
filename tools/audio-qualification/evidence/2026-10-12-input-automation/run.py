from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,os,subprocess,sys,time

root=Path(__file__).parent
name,cwd,*command=sys.argv[1:]
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
if name=='baseline':env['CARGO_TARGET_DIR']=str(root/'baseline-target')
if name=='export':
 env['DEADPAN_PREVIEW_EXPORT_KEEP']='1'
 env['DEADPAN_PREVIEW_EXPORT_RESULTS']=str(root/'export-results.json')
start=time.monotonic();now=datetime.now(timezone.utc).isoformat()
with (root/(name+'.log')).open('x') as log:
 result=subprocess.run(command,cwd=cwd,env=env,stdout=log,stderr=subprocess.STDOUT)
record=dict(name=name,cwd=cwd,command=command,started_utc=now,seconds=time.monotonic()-start,exit=result.returncode,log_sha256=hashlib.sha256((root/(name+'.log')).read_bytes()).hexdigest())
(root/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
raise SystemExit(result.returncode)
