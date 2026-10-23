from pathlib import Path
import os, subprocess, time, json
q = Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23')
(q/'qualification-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__))},indent=2)+'\n')
steps = [
 ['python',str(q/'qualify_all_ui.py'),'candidate-all-core','candidate','allcore'],
 ['python',str(q/'qualify_all_ui.py'),'candidate-broad','candidate','broad'],
 ['python',str(q/'qualify_all_ui.py'),'candidate-ui','candidate','ui'],
 ['python',str(q/'qualify_all_ui.py'),'candidate-strict','candidate','strict'],
 ['python',str(q/'ui_lint.py')],
 ['python',str(q/'qualify_native_media.py'),'debug-media','preflight'],
 ['python',str(q/'qualify_native_media.py'),'release-media','release-media'],
 ['python',str(q/'qualify_native_media.py'),'release-native','native'],
]
rows = []
for index, command in enumerate(steps,1):
 started = time.monotonic()
 with (q/f'qualification-step-{index}.stdout').open('w') as out, (q/f'qualification-step-{index}.stderr').open('w') as err:
  result = subprocess.run(command,stdout=out,stderr=err)
 rows.append({'step':index,'command':command,'exit':result.returncode,'seconds':time.monotonic()-started})
 (q/'qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if result.returncode:
  raise SystemExit(result.returncode)
