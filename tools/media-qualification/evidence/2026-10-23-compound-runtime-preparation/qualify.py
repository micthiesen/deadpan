from pathlib import Path
import os,subprocess,json,time
s=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
(s/'qualification-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__)),'stdout_stderr_redirected':True},indent=2)+'\n')
rows=json.loads((s/'focused-receipt.json').read_text())
assert rows[-1]['name']=='source-guard'and all(r['exit']==0 for r in rows)
steps=[
 ['python',str(s/'qualify_all_ui.py'),'candidate-broad','candidate','broad'],
 ['python',str(s/'qualify_all_ui.py'),'candidate-ui','candidate','ui'],
 ['python',str(s/'qualify_all_ui.py'),'candidate-strict','candidate','strict'],
 ['python',str(s/'ui_lint.py')],
 ['python',str(s/'qualify_native_media.py'),'debug-media','preflight'],
 ['python',str(s/'qualify_native_media.py'),'release-media','release-media'],
 ['python',str(s/'qualify_native_media.py'),'release-native','native'],
]
rows=[]
for i,cmd in enumerate(steps,1):
 start=time.monotonic()
 with(s/f'qualification-step-{i}.stdout').open('w')as out,(s/f'qualification-step-{i}.stderr').open('w')as err:r=subprocess.run(cmd,stdout=out,stderr=err)
 rows.append({'step':i,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
