from pathlib import Path
import subprocess,json,time
s=Path('/tmp/deadpan-neutral-ungroup-2026-10-22')
rows=[]
for phase,kind in [('candidate-pcm','pcm'),('candidate-store','store'),('candidate-all-core','allcore'),('candidate-broad','broad')]:
 start=time.monotonic()
 r=subprocess.run(['python',str(s/'qualify.py'),phase,'live',kind])
 row={'phase':phase,'kind':kind,'exit':r.returncode,'seconds':time.monotonic()-start}
 rows.append(row)
 (s/'matrix-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(row),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
