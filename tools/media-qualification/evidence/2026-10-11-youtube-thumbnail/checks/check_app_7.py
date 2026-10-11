from pathlib import Path
import os, subprocess, time, json, hashlib
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[('app-fmt-7',['cargo','fmt','--all']),('cli-thumbnail-tests-2',['cargo','nextest','run','--locked','-p','deadpan-cli','--lib','-E','test(youtube::thumbnail)']),('app-clippy-7',['cargo','clippy','--locked','-p','deadpan-app','--all-targets','--features','ui-harness','--','-D','warnings']),('app-youtube-tests-4',['cargo','nextest','run','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app','-E','test(youtube)']),('thumbnail-replay-3',['cargo','xtask','replays','--jobs','1','--scenario','youtube','--output',str(root/'thumbnail-replay-3')])]
steps.extend([('real-thumbnail',['cargo','run','--locked','-p','deadpan-cli','--example','qualify_youtube_thumbnail','--',str(root/'real-thumbnail')]),('real-app-job',['cargo','test','--locked','-p','deadpan-app','--bin','deadpan-app','youtube::tests::real_url_import_through_the_app_job_when_requested','--','--exact','--nocapture'])])
rows=[]
for name,cmd in steps:
 print('START',name,flush=True)
 start=time.monotonic(); log=root/(name+'.log')
 current_env=dict(env)
 if name=='real-app-job':
  current_env.update(DEADPAN_REAL_YOUTUBE_URL='https://youtu.be/Z4C82eyhwgU?si=share',DEADPAN_REAL_YOUTUBE_REQUIRE_THUMBNAIL='1',DEADPAN_MEDIA_WORKER=str(repo/'target/debug/deadpan-media-worker'))
 with log.open('x') as out:
  result=subprocess.run(cmd,cwd=repo,env=current_env,stdout=out,stderr=subprocess.STDOUT)
 rows.append(dict(name=name,command=cmd,exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
 (root/'app-checks-7.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,flush=True)
 if result.returncode:
  print(log.read_text(errors='replace')[-12000:],flush=True)
  raise SystemExit(result.returncode)
