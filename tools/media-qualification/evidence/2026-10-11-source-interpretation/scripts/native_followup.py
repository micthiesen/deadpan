from pathlib import Path
import os, subprocess, json, hashlib, time, re
root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps = [
 ('native-docs-generate', ['test','--locked','-p','deadpan-app','--bin','deadpan-app','commands_reference_is_current'], {'DEADPAN_UPDATE_COMMANDS_MD':'1'}),
 ('native-followup-fmt', ['fmt','--all'], {}),
 ('native-followup-registry', ['test','--locked','-p','deadpan-app','--bin','deadpan-app','navigation::registry'], {}),
 ('native-followup-playback', ['test','--locked','-p','deadpan-playback','--lib','proposed'], {}),
 ('native-followup-app', ['test','--locked','-p','deadpan-app','--bin','deadpan-app'], {}),
 ('native-followup-clippy', ['clippy','--locked','--workspace','--all-targets','--features','deadpan-app/ui-harness','--','-D','warnings'], {}),
]
rows=[]
for name,args,extra in steps:
 print('START',name,flush=True)
 start=time.monotonic(); path=root/(name+'.log')
 with path.open('w') as log:
  result=subprocess.run(['cargo',*args],env={**env,**extra},stdout=log,stderr=subprocess.STDOUT)
 log=path.read_text(); passed=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',log)))
 generated=bool(extra) and result.returncode==101 and 'rewritten; review the diff' in log
 rows.append(dict(name=name,exit=result.returncode,command=['cargo',*args],passed=passed,expected_generator_exit=generated,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
 (root/'native-followup-tests.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,'passed',passed,flush=True)
 if generated:
  diff=subprocess.run(['git','diff','--','docs/COMMANDS.md'],text=True,capture_output=True,check=True).stdout
  print(diff,flush=True)
  continue
 if result.returncode or (args[0]=='test' and passed==0):
  print(log[-18000:]); raise SystemExit(result.returncode or 1)
