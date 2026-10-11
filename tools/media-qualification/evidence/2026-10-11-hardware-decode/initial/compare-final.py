from pathlib import Path
import hashlib,json,os,subprocess,time,shutil
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
out=r/'final';out.mkdir(exist_ok=True)
binary=repo/'target/release/examples/compare_decode'
copy=out/'compare_decode';shutil.copy2(binary,copy)
source_files=['native/deadpan-source/src/decoder.c','native/deadpan-source/src/decoder.h','native/deadpan-source/src/hardware.h','native/deadpan-source/src/lib.rs','native/deadpan-source/build.rs','native/deadpan-source/tests/hardware_decode.rs','crates/deadpan-media/src/source_session.rs','crates/deadpan-media/tests/progressive_admission.rs','crates/deadpan-media/examples/compare_decode.rs']
source={}
for name in source_files:
 p=repo/name; dest=out/'source'/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,dest)
 source[name]=hashlib.sha256(p.read_bytes()).hexdigest()
identity=dict(head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),source_sha256=source,binary_sha256=hashlib.sha256(copy.read_bytes()).hexdigest())
(out/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
for name,args in [('system',['sw_vers']),('hardware',['sysctl','machdep.cpu.brand_string','hw.memsize']),('rust',['rustc','--version','--verbose']),('power-before',['pmset','-g','batt']),('thermal-before',['pmset','-g','therm'])]:
 p=subprocess.run(args,capture_output=True);(out/(name+'.txt')).write_bytes(p.stdout+p.stderr)
rows=[]
for name in ['1080p60','4k30','hevc-4k','vp9-4k']:
 args=[str(copy),str(r/(name+'.mp4')),str(out/(name+'.json'))]
 waiting=time.monotonic()
 while os.getloadavg()[0]>4.0:
  if time.monotonic()-waiting>180: raise RuntimeError('machine did not become quiet for '+name)
  print('waiting for load below 4',name,os.getloadavg(),flush=True)
  time.sleep(5)
 before=os.getloadavg(); start=time.monotonic()
 with (out/(name+'.log')).open('x') as log:
  p=subprocess.run(['/usr/bin/time','-l',*args],stdout=log,stderr=subprocess.STDOUT,timeout=600)
 row=dict(command=args,exit=p.returncode,seconds=time.monotonic()-start,load_before=before,load_after=os.getloadavg(),binary_sha256=identity['binary_sha256'])
 rows.append(row);print(name,row,flush=True)
 (out/'runs.json').write_text(json.dumps(rows,indent=2)+'\n')
 assert p.returncode==0
for name,args in [('power-after',['pmset','-g','batt']),('thermal-after',['pmset','-g','therm'])]:
 p=subprocess.run(args,capture_output=True);(out/(name+'.txt')).write_bytes(p.stdout+p.stderr)
