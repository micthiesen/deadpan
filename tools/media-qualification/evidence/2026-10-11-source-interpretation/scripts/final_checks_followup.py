from pathlib import Path
import os, subprocess, json, hashlib, time, re
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
binary=repo/'target/debug/deadpan-app'
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix', PATH=str(binary.parent)+os.pathsep+os.environ['PATH'])
steps=[
 ('final-store-followup',['cargo','test','--locked','-p','deadpan-store']),
 ('final-media',['cargo','test','--locked','-p','deadpan-media']),
 ('final-host-libraries',['cargo','test','--locked','-p','deadpan-cli','-p','deadpan-models','-p','deadpan-playback','-p','deadpan-jobs','--lib']),
 ('final-cli-contracts',['cargo','test','--locked','-p','deadpan-cli','--test','project_commands','--test','bridge_conditioning','--test','shots','--test','proxy_resume']),
 ('final-model-python',['python3','-m','unittest','discover','-s','tools/model-qualification/tests']),
 ('final-ui-build',['cargo','build','--locked','-p','deadpan-app','--features','ui-harness']),
 ('final-original-layout',['deadpan-app','--ui-check','--scenario','original-layout','--mode','visual','--output',str(root/'final-original-layout'),'--retain-projects']),
 ('final-original-layout-long',['deadpan-app','--ui-check','--scenario','original-layout-long','--mode','visual','--output',str(root/'final-original-layout-long'),'--retain-projects']),
 ('final-release',['cargo','build','--release','--locked','--workspace']),
 ('final-bundle',['cargo','xtask','bundle','--output','/tmp/deadpan-source-interpretation-bundle-20261011']),
 ('final-bundle-verify',['cargo','xtask','bundle-verify','/tmp/deadpan-source-interpretation-bundle-20261011/Deadpan.app']),
 ('final-fmt',['cargo','fmt','--all','--','--check']),
 ('final-clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','deadpan-app/ui-harness','--','-D','warnings']),
]
rows=[]
for name,command in steps:
 print('START',name,flush=True)
 start=time.monotonic(); path=root/(name+'.log')
 with path.open('w') as log:
  result=subprocess.run(command,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
 output=path.read_text(); passed=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',output)))
 row=dict(name=name,exit=result.returncode,command=command,passed=passed,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
 if command[0]=='deadpan-app': row['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
 rows.append(row)
 (root/'final-checks-followup.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,'passed',passed,flush=True)
 if result.returncode:
  print(output[-18000:]); raise SystemExit(result.returncode)
