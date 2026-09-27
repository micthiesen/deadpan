"""Verify the CLI diagnostic correction after the complete, unchanged workspace gate."""
import hashlib,json,os,re,subprocess,time
from pathlib import Path
r=Path('/Users/michael/Code/deadpan'); s=Path('/tmp/deadpan-retime-20260927')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
assert (s/'gate-1/report.json').exists()
pre=json.loads((s/'gate-1/source-before-tests.json').read_text())
assert json.loads((s/'gate-1/source-after.json').read_text())==pre
expected={p:hashlib.sha256((r/p).read_bytes()).hexdigest() for p in pre}
changed=[p for p in expected if expected[p]!=pre[p]]
assert set(changed)=={'crates/deadpan-cli/src/doctor.rs','crates/deadpan-cli/tests/project_commands.rs'},changed
(s/'final-source.json').write_text(json.dumps(expected,indent=2)+'\n')
commands=[
 ['cargo','fmt','--all','--','--check'],
 ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings'],
 ['cargo','test','-p','deadpan-cli','--test','project_commands','--locked'],
 ['cargo','run','-p','deadpan-cli','--locked','--','doctor'],
 ['cargo','run','-p','deadpan-app','--features','ui-harness','--locked','--','--ui-check','--scenario','retime','--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift','--output',str(s/'ui-retime')],
]
out=s/'final-checks'; out.mkdir(exist_ok=False)
report={'commands':[],'changes_after_workspace_gate':changed}
for i,cmd in enumerate(commands):
 print(json.dumps({'started':i,'command':cmd}),flush=True); start=time.monotonic()
 with (out/f'{i}.log').open('w') as log:
  result=subprocess.run(cmd,cwd=r,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'command':cmd,'exit_code':result.returncode,'seconds':time.monotonic()-start,'log':f'{i}.log'}
 tests=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', (out/f'{i}.log').read_text())
 if tests: row['tests']=dict(zip(('passed','failed','ignored'),(sum(int(t[j]) for t in tests) for j in range(3))))
 report['commands'].append(row)
 (out/'progress.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(row),flush=True)
 if result.returncode and i<4: break
report['sources_match_final_seal']=all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in expected.items())
(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
