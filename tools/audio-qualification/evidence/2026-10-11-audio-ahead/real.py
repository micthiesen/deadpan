from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
perf=repo/'target/release/examples/perf'
pins={str(p.relative_to(repo)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [perf,repo/'target/release/deadpan-cli']}
(root/'release-binaries.json').write_text(json.dumps(pins,indent=2)+'\n')
for name,extra in [('real-once',['--seconds','20']),('real-loop',['--seconds','35','--loop-end','480'])]:
 processes=subprocess.check_output(['ps','-Ao','pid,pcpu,comm'],text=True)
 compilers=[line for line in processes.splitlines() if line.rstrip().endswith(('/rustc','/cargo','/clang'))]
 if compilers:raise RuntimeError('Wait for compilers before measuring: '+str(compilers))
 (root/(name+'-host.json')).write_text(json.dumps(dict(load=os.getloadavg(),perf_sha256=pins[str(perf.relative_to(repo))]),indent=2)+'\n')
 command=['/usr/bin/time','-l',str(perf),'playback',str(root/'future.deadpan'),'--pictures','original',*extra]
 start=time.monotonic()
 with (root/(name+'-report.json')).open('x') as out,(root/(name+'.stderr')).open('x') as err:
  result=subprocess.run(command,cwd=repo,stdout=out,stderr=err)
 record=dict(command=command,exit=result.returncode,seconds=time.monotonic()-start,stdout_sha256=hashlib.sha256((root/(name+'-report.json')).read_bytes()).hexdigest(),stderr_sha256=hashlib.sha256((root/(name+'.stderr')).read_bytes()).hexdigest())
 (root/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n')
 print(record,flush=True)
 if result.returncode:raise SystemExit(result.returncode)
 report=json.loads((root/(name+'-report.json')).read_text())
 print(json.dumps({k:report[k] for k in ['covered_requested_interval','failure','heard_seconds','audio']}),flush=True)
 if report['failure'] or not report['covered_requested_interval']:raise SystemExit(2)
