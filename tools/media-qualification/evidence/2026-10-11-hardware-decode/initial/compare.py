from pathlib import Path
import hashlib,json,os,subprocess,time,shutil
r=Path(__file__).parent
binary=Path('/Users/michael/Code/deadpan/target/release/examples/compare_decode')
copy=r/'compare-v1';shutil.copy2(binary,copy)
rows=[]
for name in ['1080p60','4k30']:
 args=[str(copy),str(r/(name+'.mp4')),str(r/(name+'.compare.json'))]
 start=time.monotonic()
 with (r/(name+'.compare.log')).open('x') as log:
  p=subprocess.run(['/usr/bin/time','-l',*args],stdout=log,stderr=subprocess.STDOUT,timeout=600)
 row=dict(command=args,exit=p.returncode,seconds=time.monotonic()-start,binary_sha256=hashlib.sha256(copy.read_bytes()).hexdigest())
 rows.append(row);print(row,flush=True)
 assert p.returncode==0
(r/'compare-runs.json').write_text(json.dumps(rows,indent=2)+'\n')
