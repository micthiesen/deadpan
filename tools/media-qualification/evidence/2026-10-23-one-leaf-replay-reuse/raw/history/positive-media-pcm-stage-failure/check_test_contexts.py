from pathlib import Path
import subprocess,os,json,time
q=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23')
(q/'test-context-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__))},indent=2)+'\n')
subprocess.run(['python',str(q/'freeze.py'),'test-context'],check=True)
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_PROFILE_DEV_OPT_LEVEL='2')
command=['cargo','check','--locked','-p','deadpan-cli','--tests','--features','synthetic-worker']
start=time.monotonic()
with(q/'test-context-check.log').open('w') as out:
 result=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
(q/'test-context-check-receipt.json').write_text(json.dumps({'command':command,'exit':result.returncode,'seconds':time.monotonic()-start,'scope':'Typecheck every CLI test compilation context; no test execution claim'},indent=2)+'\n')
subprocess.run(['python',str(q/'guard.py'),'test-context'],check=True)
raise SystemExit(result.returncode)
