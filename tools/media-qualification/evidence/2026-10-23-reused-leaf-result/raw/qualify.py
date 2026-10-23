from pathlib import Path
import os,subprocess,time,json
s=Path('/tmp/deadpan-reused-leaf-result-qualification-2026-10-23')
(s/'qualification-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__)),'waiting_for':'focused four steps, selected one test'},indent=2)+'\n')
while True:
 try:rows=json.loads((s/'focused-receipt.json').read_text())
 except(FileNotFoundError,json.JSONDecodeError):rows=[]
 if any(r['exit']!=0 for r in rows):raise SystemExit('focused check failed; no further checks started')
 if len(rows)==4:break
 time.sleep(2)
assert 'running 1 test' in(s/'focused-3.stdout').read_text()
steps=[
 ['python',str(s/'qualify_all_ui.py'),'candidate-all-core','candidate','allcore'],
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
 (s/'qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
