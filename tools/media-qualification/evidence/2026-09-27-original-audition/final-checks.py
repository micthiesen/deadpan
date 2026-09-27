"""Recheck lint after review changes, then attempt the production GUI replay."""
import hashlib,json,os,subprocess,time
from pathlib import Path
r=Path('/Users/michael/Code/deadpan');s=Path('/tmp/deadpan-original-audition-20260927')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
while not (s/'gate-2/report.json').exists():
 time.sleep(5)
expected=json.loads((s/'stable-before-tests.json').read_text())['source_sha256']
assert all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in expected.items())
commands=[['cargo','fmt','--all','--','--check'],['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings'],['cargo','run','-p','deadpan-app','--features','ui-harness','--locked','--','--ui-check','--scenario','original-playback','--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift','--output',str(s/'ui-original-playback')]]
out=s/'final-checks';out.mkdir(exist_ok=False);report={'commands':[]}
for i,cmd in enumerate(commands):
 print(json.dumps({'started':i,'command':cmd}),flush=True);start=time.monotonic()
 with (out/f'{i}.log').open('w') as log:
  result=subprocess.run(cmd,cwd=r,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'command':cmd,'exit_code':result.returncode,'seconds':time.monotonic()-start,'log':f'{i}.log'}
 report['commands'].append(row);(out/'progress.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(row),flush=True)
 if result.returncode and i<2:break
report['sources_match_pretest_seal']=all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in expected.items())
(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
