from pathlib import Path
import os,subprocess,json,time,hashlib
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
(s/'final-driver-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__)),'stdout_stderr_redirected':True},indent=2)+'\n')
steps=[
 ['python',str(s/'qualify.py'),'candidate-broad','candidate','broad'],
 ['python',str(s/'qualify.py'),'candidate-ui','candidate','ui'],
 ['python',str(s/'qualify.py'),'candidate-strict','candidate','strict'],
 ['python',str(s/'ui_lint.py')],
 ['python',str(s/'qualify_native_media.py'),'debug-media','preflight'],
 ['python',str(s/'qualify_native_media.py'),'release-media','release-media'],
 ['python',str(s/'qualify_native_media.py'),'release-native','native'],
]
rows=[]
for i,command in enumerate(steps):
 start=time.monotonic()
 with(s/f'final-step-{i+1}.stdout').open('w')as out,(s/f'final-step-{i+1}.stderr').open('w')as err:
  result=subprocess.run(command,stdout=out,stderr=err)
 rows.append({'command':command,'exit':result.returncode,'seconds':time.monotonic()-start})
 (s/'final-qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
