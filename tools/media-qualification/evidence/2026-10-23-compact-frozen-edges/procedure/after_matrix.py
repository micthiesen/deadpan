from pathlib import Path
import subprocess,json,time
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
expected='/tmp/deadpan-compact-frozen-edges-2026-10-22/run_matrix.py'
start='Thu Oct 22 22:24:01 2026'
while True:
 r=subprocess.run(['ps','-p','85560','-o','lstart=,args='],capture_output=True,text=True)
 if r.returncode or expected not in r.stdout or start not in r.stdout:break
 time.sleep(5)
rows=json.loads((s/'matrix-final-receipt.json').read_text())
assert len(rows)==5,rows
assert [r['exit']for r in rows]==[0,100,0,0,0],rows
steps=[['python',str(s/'finish_qualification.py')],['python',str(s/'prepare_performance.py')],['python',str(s/'measure.py')]]
out=[]
for command in steps:
 began=time.monotonic()
 with(s/('after-matrix-'+str(len(out)+1)+'.log')).open('w')as log:
  result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
 out.append({'command':command,'exit':result.returncode,'seconds':time.monotonic()-began})
 (s/'after-matrix-receipt.json').write_text(json.dumps(out,indent=2)+'\n')
 print(json.dumps(out[-1]),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
subprocess.run(['python',str(s/'guard.py'),'release-perf'],check=True)
