from pathlib import Path
import subprocess,time,json,os
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
subprocess.run(['python',str(s/'guard.py'),'release-perf'],check=True)
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
args=['cargo','clippy','--locked','-p','deadpan-app','--features','ui-harness','--all-targets','--','-D','warnings']
start=time.monotonic()
with(s/'ui-lint.stdout').open('w')as out,(s/'ui-lint.stderr').open('w')as err:
 r=subprocess.run(args,cwd='/Users/michael/Code/deadpan',env=env,stdout=out,stderr=err)
(s/'ui-lint-receipt.json').write_text(json.dumps({'command':args,'exit':r.returncode,'seconds':time.monotonic()-start,'scope_correction':'Initial xtask check covered only changed Core/Store packages. This explicitly supplies the UI Clippy evidence.'},indent=2)+'\n')
subprocess.run(['python',str(s/'guard.py'),'release-perf'],check=True)
raise SystemExit(r.returncode)
