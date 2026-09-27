import hashlib, json, os, subprocess, sys, time
from pathlib import Path
repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-interior-splice-20260926')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
commands=[
    ['cargo','run','-p','deadpan-app','--features','ui-harness','--locked','--','--ui-check','--scenario','editing','--output',str(scratch/'ui-editing')],
    ['python3',str(scratch/'durable-cli.py'),'target/debug/deadpan-app',str(scratch/'durable-app')],
    ['cargo','test','--locked','-p','deadpan-plan','-p','deadpan-store'],
]
records=[]
for i,cmd in enumerate(commands):
    start=time.monotonic()
    with (scratch/f'app-check-{i}.log').open('w') as log:
        result=subprocess.run(cmd,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
    row=dict(command=cmd,exit_code=result.returncode,seconds=time.monotonic()-start,log=f'app-check-{i}.log',binary_sha256=hashlib.sha256((repo/'target/debug/deadpan-app').read_bytes()).hexdigest())
    records.append(row)
    (scratch/'app-checks.json').write_text(json.dumps(records,indent=2)+'\n')
    print(json.dumps(row),flush=True)
sys.exit(0 if all(r['exit_code']==0 for r in records) else 1)
