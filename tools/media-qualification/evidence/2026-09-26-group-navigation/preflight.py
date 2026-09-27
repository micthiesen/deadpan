import json
import os
from pathlib import Path
import subprocess
import time

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
commands=[
 ['cargo','clippy','-p','deadpan-app','--features','ui-harness','--all-targets','--locked','--','-D','warnings'],
 ['cargo','test','-p','deadpan-app','--features','ui-harness','--all-targets','--locked'],
]
report=[]
for index,command in enumerate(commands):
    start=time.monotonic()
    with (scratch/f'preflight-{index}.log').open('w') as log:
        result=subprocess.run(command,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
    row=dict(command=command,exit_code=result.returncode,seconds=time.monotonic()-start)
    report.append(row)
    print(json.dumps(row),flush=True)
    if result.returncode: break
(scratch/'preflight.json').write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(report[-1]['exit_code'])
