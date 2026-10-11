from pathlib import Path
import os, subprocess, time, json, hashlib
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[
 ('final-fmt',['cargo','fmt','--all']),
 ('isolated-cancellation',['cargo','nextest','run','--locked','-p','deadpan-cli','--lib','--test-threads','1','-E','test(youtube::thumbnail::tests::cancellation_before_or_during_fetch_never_reaches_confirmation)']),
 ('real-thumbnail-2',['cargo','run','--locked','-p','deadpan-cli','--example','qualify_youtube_thumbnail','--',str(root/'real-thumbnail')]),
 ('real-app-job',['cargo','test','--locked','-p','deadpan-app','--bin','deadpan-app','youtube::tests::real_url_import_through_the_app_job_when_requested','--','--exact','--nocapture']),
 ('milestone-gate',['cargo','xtask','gate']),
 ('affected-replays',['cargo','xtask','replays','--jobs','2','--scenario','linked-original,layouts','--output',str(root/'affected-replays')]),
 ('bundle',['cargo','xtask','bundle','--output','/tmp/deadpan-youtube-thumbnail-bundle-20261011']),
 ('bundle-verify',['cargo','xtask','bundle-verify','/tmp/deadpan-youtube-thumbnail-bundle-20261011/Deadpan.app']),
]
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
 (root/'final-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,flush=True)
 if result.returncode:
  print(log.read_text(errors='replace')[-12000:],flush=True)
  raise SystemExit(result.returncode)
