import hashlib, json, os, re, subprocess, time
from pathlib import Path
repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-interior-splice-20260926')
out=scratch/'post-validation'
out.mkdir(exist_ok=False)
old=json.loads((scratch/'gate-1/source-after.json').read_text())
def seal():
    return {p:hashlib.sha256((repo/p).read_bytes()).hexdigest() for p in old}
before=seal()
changed=[p for p in old if before[p]!=old[p]]
assert changed==['crates/deadpan-core/tests/audio_bindings/gaps.rs'],changed
(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
commands=[
    ['cargo','fmt','--all','--','--check'],
    ['cargo','clippy','-p','deadpan-core','--all-targets','--locked','--','-D','warnings'],
    ['cargo','test','--locked','-p','deadpan-core','--test','audio_bindings'],
    ['cargo','test','--workspace','--locked'],
]
records=[]
for i,cmd in enumerate(commands):
    print(json.dumps(dict(started=i,command=cmd)),flush=True)
    start=time.monotonic()
    with (out/f'{i}.log').open('w') as log:
        result=subprocess.run(cmd,cwd=repo,env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix'),stdout=log,stderr=subprocess.STDOUT)
    rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',(out/f'{i}.log').read_text())
    record=dict(command=cmd,exit_code=result.returncode,seconds=time.monotonic()-start,log=f'{i}.log')
    if rows: record['tests']=dict(zip(('passed','failed','ignored'),(sum(int(r[j]) for r in rows) for j in range(3))))
    records.append(record)
    print(json.dumps(record),flush=True)
    (out/'progress.json').write_text(json.dumps(records,indent=2)+'\n')
    if result.returncode and i<3: break
after=seal()
(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
report=dict(commands=records,source_count=len(after),source_unchanged=before==after,changed_since_original_gate=changed,production_sources_unchanged=True)
(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
