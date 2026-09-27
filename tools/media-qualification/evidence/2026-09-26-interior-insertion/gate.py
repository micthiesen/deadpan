"""Required exact gate, with a separate run of the targets after jobs' denied socket test."""
import hashlib, json, os, re, subprocess, sys, time
from pathlib import Path
repo = Path('/Users/michael/Code/deadpan')
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=False)
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
commands = [
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
    ['cargo', 'test', '--workspace', '--locked'],
    ['cargo', 'build', '--workspace', '--locked'],
    ['cargo', 'run', '-p', 'deadpan-cli', '--', 'doctor'],
    ['cargo', 'test', '--locked', '-p', 'deadpan-plan', '--test', 'picture_plan'],
    ['cargo', 'clippy', '-p', 'deadpan-app', '--features', 'ui-harness', '--all-targets', '--locked', '--', '-D', 'warnings'],
    ['cargo', 'test', '-p', 'deadpan-app', '--features', 'ui-harness', '--all-targets', '--locked'],
]
def seal():
    paths = subprocess.check_output(['git','-c','core.fsmonitor=false','ls-files','-z','--cached','--others','--exclude-standard'], cwd=repo).decode().split('\0')
    return {p:hashlib.sha256((repo/p).read_bytes()).hexdigest() for p in sorted(set(paths)) if p and (p.startswith(('crates/','native/')) or p in ('Cargo.toml','Cargo.lock','rust-toolchain.toml','rustfmt.toml')) and (repo/p).is_file()}
before = seal()
(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
report = dict(commands=[], source_count=len(before))
for index,command in enumerate(commands):
    print(json.dumps(dict(started=index,command=command)),flush=True)
    start=time.monotonic()
    with (out/f'{index}.log').open('w') as log:
        result=subprocess.run(command,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
    rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', (out/f'{index}.log').read_text())
    record=dict(command=command,exit_code=result.returncode,seconds=time.monotonic()-start,log=f'{index}.log')
    if rows: record['tests']=dict(zip(('passed','failed','ignored'),(sum(int(r[i]) for r in rows) for i in range(3))))
    report['commands'].append(record)
    (out/'progress.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(record),flush=True)
    if result.returncode and index in (0,1): break
after=seal()
(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
report['source_unchanged']=before==after
report['changed_sources']=[p for p in sorted(set(before)|set(after)) if before.get(p)!=after.get(p)]
report['passed']=len(report['commands'])==len(commands) and all(c['exit_code']==0 for c in report['commands']) and before==after
(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
sys.exit(0 if report['passed'] else 1)
