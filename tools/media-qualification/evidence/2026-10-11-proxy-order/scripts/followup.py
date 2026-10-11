from pathlib import Path
import os,json,subprocess,time,hashlib
root=Path(__file__).parent;repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[('warning-followup',['cargo','nextest','run','--locked','-p','deadpan-media-worker','--test','proxy_real_media','--test-threads','1','-E','test(downscaled_rotated_anamorphic_and_wide_gamut_originals_keep_their_interpretation) | test(an_assembly_refuses_a_misplaced_or_damaged_range)']),('retry-stability',['cargo','nextest','run','--locked','-p','deadpan-media-worker','--test','proxy_real_media','--stress-count','20','-E','test(a_stalled_worker_is_torn_down_and_retried_once_in_a_fresh_output)']),('fmt-check',['cargo','fmt','--all','--','--check'])]
rows=[]
for name,cmd in steps:
 print('START',name,flush=True);start=time.monotonic();path=root/(name+'.log')
 with path.open('x') as log:result=subprocess.run(cmd,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
 rows.append(dict(name=name,command=cmd,exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
 (root/'followup-checks.json').write_text(json.dumps(rows,indent=2)+'\n');print('FINISH',name,result.returncode,flush=True)
 if result.returncode:print(path.read_text()[-9000:]);raise SystemExit(result.returncode)
