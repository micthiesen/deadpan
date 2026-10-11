from pathlib import Path
import hashlib,json,os,subprocess,time,shutil
r=Path(__file__).parent
copy=r/'compare-v1'
rows=[]
for name in ['hevc-4k','vp9-4k']:
 args=[str(copy),str(r/(name+'.mp4')),str(r/(name+'.compare.json'))]
 start=time.monotonic()
 with (r/(name+'.compare.log')).open('x') as log:
  p=subprocess.run(['/usr/bin/time','-l',*args],stdout=log,stderr=subprocess.STDOUT,timeout=600)
 row=dict(command=args,exit=p.returncode,seconds=time.monotonic()-start,binary_sha256=hashlib.sha256(copy.read_bytes()).hexdigest())
 rows.append(row);print(row,flush=True)
 assert p.returncode==0
(r/'compare-codecs-runs.json').write_text(json.dumps(rows,indent=2)+'\n')
