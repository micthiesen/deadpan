from pathlib import Path
import os,subprocess,time,json
q=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23')
(q/'qualification-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__)),'scope':'test-only PCM reader source amendment; strict and media controls rerun'},indent=2)+'\n')
prior=json.loads((q/'qualification-receipt.json').read_text())
assert len(prior)==7 and prior[-1]['step']==7 and prior[-1]['exit']==100
assert all(row['exit']==0 for row in prior[:6])
rows=prior[:6]
(q/'qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
steps=[
 (1,['python',str(q/'qualify_all_ui.py'),'candidate-strict','candidate','strict']),
 (6,['python',str(q/'qualify_native_media.py'),'debug-media','preflight']),
 (7,['python',str(q/'qualify_native_media.py'),'release-media','release-media']),
 (8,['python',str(q/'qualify_native_media.py'),'release-native','native']),
]
for index,command in steps:
 started=time.monotonic()
 with(q/f'qualification-step-{index}.stdout').open('w') as out,(q/f'qualification-step-{index}.stderr').open('w') as err:
  result=subprocess.run(command,stdout=out,stderr=err)
 row={'step':index,'command':command,'exit':result.returncode,'seconds':time.monotonic()-started}
 rows=[old for old in rows if old['step']!=index]+[row];rows.sort(key=lambda r:r['step'])
 (q/'qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(row),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
