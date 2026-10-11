from pathlib import Path
import os, subprocess, json, hashlib, time, re
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
summary=json.loads((root/'milestone-replays/summary.json').read_text())
scenarios=['transcript','source-interpretation','original-layout','original-layout-long','layouts']
scenarios.extend(s['name'] for s in summary['scenarios'] if s['status']=='FAIL' and s['name'] not in scenarios)
steps=[
 ('rail-fmt-followup',['cargo','fmt','--all','--','--check']),
 ('rail-base-clippy',['cargo','clippy','--locked','-p','deadpan-app','--all-targets','--','-D','warnings']),
 ('rail-harness-clippy-followup',['cargo','clippy','--locked','-p','deadpan-app','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('rail-base-tests-followup',['cargo','nextest','run','--locked','-p','deadpan-app','--bin','deadpan-app','-E','test(transcript) | test(interpretation)']),
 ('rail-harness-tests',['cargo','nextest','run','--locked','-p','deadpan-app','--bin','deadpan-app','--features','ui-harness','-E','test(transcript) | test(interpretation)']),
 ('rail-replays-followup',['cargo','xtask','replays','--jobs','2','--scenario','transcript,original-layout,original-layout-long','--output',str(root/'rail-replays-followup')]),
 ('rail-performance-build',['cargo','build','--release','--locked','-p','deadpan-app','--features','ui-harness']),
 ('rail-performance',['deadpan-app','--ui-check','--scenario','source-interpretation','--mode','performance','--output',str(root/'rail-performance')]),
 ('rail-bundle',['cargo','xtask','bundle','--output','/tmp/deadpan-source-interpretation-final-bundle-20261011']),
 ('rail-bundle-verify',['cargo','xtask','bundle-verify','/tmp/deadpan-source-interpretation-final-bundle-20261011/Deadpan.app']),
]
record=root/'rail-checks.json'
rows=json.loads(record.read_text()) if record.exists() else []
for name,command in steps:
 if any(row['name']==name and row['exit']==0 for row in rows): continue
 print('START',name,flush=True)
 start=time.monotonic(); path=root/(name+'.log')
 current_env=dict(env)
 if command[0]=='deadpan-app': current_env['PATH']=str(repo/'target/release')+os.pathsep+current_env['PATH']
 with path.open('x') as log:
  result=subprocess.run(command,cwd=repo,env=current_env,stdout=log,stderr=subprocess.STDOUT)
 row=dict(name=name,exit=result.returncode,command=command,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
 if command[0]=='deadpan-app': row['binary_sha256']=hashlib.sha256((repo/'target/release/deadpan-app').read_bytes()).hexdigest()
 rows.append(row)
 (root/'rail-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,flush=True)
 if result.returncode:
  print(path.read_text(errors='replace')[-15000:],flush=True)
  raise SystemExit(result.returncode)
