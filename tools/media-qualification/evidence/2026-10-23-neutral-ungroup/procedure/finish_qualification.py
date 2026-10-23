from pathlib import Path
import subprocess,json,time
s=Path('/tmp/deadpan-neutral-ungroup-2026-10-22')
rows=[]
steps=[
 ['python',str(s/'qualify.py'),'candidate-ui','live','ui'],
 ['python',str(s/'qualify_native_media.py'),'debug-media','preflight'],
 ['python',str(s/'qualify_native_media.py'),'release-media','release-media'],
 ['python',str(s/'qualify_native_media.py'),'release-native','native'],
]
for command in steps:
 start=time.monotonic();r=subprocess.run(command)
 rows.append({'command':command,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'finish-qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
