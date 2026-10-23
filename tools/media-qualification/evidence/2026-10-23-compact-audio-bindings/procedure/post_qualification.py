from pathlib import Path
import subprocess,json,time,os,hashlib
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
(s/'post-driver-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__)),'waits_for':'final-qualification-2-receipt.json seven exit0 rows','stdout_stderr_redirected':True},indent=2)+'\n')
while True:
 f=s/'final-qualification-2-receipt.json'
 try:rows=json.loads(f.read_text())
 except (FileNotFoundError,json.JSONDecodeError):rows=[]
 if any(row['exit']!=0 for row in rows):
  raise SystemExit('main qualification failed; post steps not started')
 if len(rows)==7:break
 time.sleep(2)
steps=[['python',str(s/'retain_media_and_check.py')],['python',str(s/'prepare_performance.py')],['python',str(s/'measure.py')],['python',str(s/'validate_performance_packages.py')]]
rows=[]
for i,cmd in enumerate(steps):
 start=time.monotonic()
 with(s/f'post-step-{i+1}.stdout').open('w')as stdout,(s/f'post-step-{i+1}.stderr').open('w')as stderr:r=subprocess.run(cmd,stdout=stdout,stderr=stderr)
 rows.append({'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'post-qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
