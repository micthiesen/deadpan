from pathlib import Path
import os, subprocess, time, json, hashlib
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=[
 ('followup-fmt',['cargo','fmt','--all','--','--check']),
 ('followup-workspace-clippy',['cargo','clippy','--workspace','--all-targets','--features','deadpan-cli/synthetic-worker','--locked','--','-D','warnings']),
 ('followup-harness-clippy',['cargo','clippy','-p','deadpan-app','--all-targets','--features','ui-harness','--locked','--','-D','warnings']),
 ('followup-cli',['cargo','nextest','run','--locked','-p','deadpan-cli','--lib','--test','privacy_network_audit','--test','privacy_sandbox','-E','test(thumbnail) | binary(privacy_network_audit) | binary(privacy_sandbox)']),
 ('followup-proxy',['cargo','nextest','run','--locked','-p','deadpan-media-worker','--test','proxy_real_media','--test-threads','1','-E','test(a_stalled_worker_is_torn_down_and_retried_once_in_a_fresh_output)']),
 ('followup-ui-tests',['cargo','nextest','run','-p','deadpan-app','--features','ui-harness','--locked','--no-fail-fast']),
 ('followup-docs',['cargo','test','--workspace','--features','deadpan-cli/synthetic-worker','--locked','--doc','--no-fail-fast']),
 ('affected-replays',['cargo','xtask','replays','--jobs','2','--scenario','youtube,linked-original,layouts','--output',str(root/'affected-replays')]),
 ('bundle',['cargo','xtask','bundle','--output','/tmp/deadpan-youtube-thumbnail-bundle-20261011']),
 ('bundle-verify',['cargo','xtask','bundle-verify','/tmp/deadpan-youtube-thumbnail-bundle-20261011/Deadpan.app']),
 ('release-thumbnail',['cargo','run','--release','--locked','-p','deadpan-cli','--example','qualify_youtube_thumbnail','--',str(root/'release-thumbnail')]),
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
 (root/'followup-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,flush=True)
 if result.returncode:
  print(log.read_text(errors='replace')[-12000:],flush=True)
  raise SystemExit(result.returncode)
