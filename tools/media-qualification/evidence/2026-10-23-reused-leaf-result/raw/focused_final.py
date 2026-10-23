from pathlib import Path
import os,subprocess,time,json
s=Path('/tmp/deadpan-reused-leaf-result-qualification-2026-10-23')
(s/'focused-final-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__))},indent=2)+'\n')
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[
 ['cargo','fmt','--all'],
 ['python',str(s/'freeze.py'),'focused-final'],
 ['cargo','test','--locked','-p','deadpan-core','--lib','interior_pause_and_audio_setter_match_legacy_leaf_patches_with_bound_audio_and_marks'],
 ['python',str(s/'guard.py'),'focused-final'],
]
rows=[]
for i,cmd in enumerate(steps,1):
 start=time.monotonic()
 with(s/f'focused-final-{i}.stdout').open('w')as out,(s/f'focused-final-{i}.stderr').open('w')as err:r=subprocess.run(cmd,env=env,stdout=out,stderr=err)
 rows.append({'step':i,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'focused-final-receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
 if i==3:assert 'running 1 test' in(s/'focused-final-3.stdout').read_text(),'new test was not selected'
