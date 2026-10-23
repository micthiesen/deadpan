from pathlib import Path
import os,subprocess,time,json
s=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
(s/'corrected-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__))},indent=2)+'\n')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_PROFILE_DEV_OPT_LEVEL='2')
steps=[
 ['cargo','fmt','--all'],
 ['python',str(s/'qualify_all_ui.py'),'candidate-ui-corrected','candidate','ui'],
 ['python',str(s/'ui_lint_corrected.py')],
 ['python',str(s/'qualify_native_media.py'),'release-native-corrected','native'],
]
rows=[]
for i,cmd in enumerate(steps,1):
 start=time.monotonic()
 with(s/f'corrected-step-{i}.stdout').open('w')as out,(s/f'corrected-step-{i}.stderr').open('w')as err:r=subprocess.run(cmd,env=env,stdout=out,stderr=err)
 rows.append({'step':i,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'corrected-receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
