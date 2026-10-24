from pathlib import Path
import os,subprocess,json
scratch=Path('/tmp/deadpan-point-targets-2026-10-24')
env=os.environ.copy()
env['CARGO_TARGET_DIR']='/tmp/deadpan-follow-creep-2026-10-24/target'
env['DEADPAN_FFMPEG_PREFIX']=str(Path.home()/'Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
results=[]
for i,args in enumerate([
['-p','deadpan-core','--lib','target::tests'],
['-p','deadpan-analysis','--lib','tracking::tests'],
['-p','deadpan-models','--lib','conditioning_region::tests'],
],1):
 cmd=['cargo','test','--locked']+args
 stem=f'worker-point-regressions-{i}'
 with (scratch/(stem+'.stdout')).open('w') as out,(scratch/(stem+'.stderr')).open('w') as err:
  result=subprocess.run(cmd,cwd='/Users/michael/Code/deadpan',env=env,stdout=out,stderr=err)
 results.append({'command':cmd,'exit':result.returncode,'stdout':stem+'.stdout','stderr':stem+'.stderr'})
 (scratch/'worker-point-regression-results.json').write_text(json.dumps(results,indent=2)+'\n')
 print(i,result.returncode,' '.join(cmd),flush=True)
 print((scratch/(stem+'.stdout')).read_text()[-1500:],flush=True)
 if result.returncode:
  print((scratch/(stem+'.stderr')).read_text()[-4000:],flush=True)
  raise SystemExit(result.returncode)
