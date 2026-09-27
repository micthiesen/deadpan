"""Retain exact validation evidence and preserve all pending work without Git writes."""
from pathlib import Path
import gzip, hashlib, json, re, shutil, subprocess
r=Path('/Users/michael/Code/deadpan')
s=Path('/tmp/deadpan-sound-events-20260927')
e=r/'tools/media-qualification/evidence/2026-09-27-scoped-mix'
e.mkdir(parents=True,exist_ok=True)
report=json.loads((s/'gate-01/report.json').read_text())
assert report['source_unchanged'], report['changed_sources']
source=json.loads((s/'gate-01/source-after.json').read_text())
assert all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in source.items())
(s/'final-source.json').write_text(json.dumps(source,indent=2)+'\n')
checks=[]
for folder in ['plan-targeted-01','audio-targeted-01','audio-targeted-02','targeted-03','audio-targeted-04','gate-01','playback-retry','playback-isolated','playback-serial']:
    dest=e/folder;dest.mkdir(exist_ok=True)
    for p in sorted((s/folder).iterdir()):
        if p.suffix=='.log':
            (dest/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
        elif p.suffix=='.json':
            shutil.copyfile(p,dest/p.name)
    d=json.loads((s/folder/'report.json').read_text())
    if 'commands' not in d:
        log=(s/folder/'test.log').read_text()
        rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)
        d['tests']=dict(zip(('passed','failed','ignored'),(sum(int(x[i]) for x in rows) for i in range(3))))
    checks.append({'run':folder, 'report':d})
(e/'policy-diagnostic.log.gz').write_bytes(gzip.compress((s/'policy-diagnostic.log').read_bytes(),mtime=0))
(e/'README.md').write_text('''# Scoped mix preparation evidence

See [the qualification record](../../../../docs/qualification/scoped-mix-2026-09-27.md)
for scope, review corrections, failures and unverified product work.

- `verification.json` summarizes exact invocations and outcomes. Earlier failed
  PCM runs and the diagnostic policy query are retained, not counted as passes.
- `gate-01` retains formatting, strict workspace lint, workspace tests/build,
  doctor, strict harness-feature lint and app/harness tests. Logs use gzip.
- `playback-retry`, `playback-isolated` and `playback-serial` retain the unchanged
  playback binary's default-thread, individual-case and single-thread diagnostic
  runs. Diagnostic passes do not replace the failed default workspace gate.
- `review.json`, `increment.json` and `increment.diff.gz` identify the reviewed
  increment against the saved pre-sound working tree. `final-source.json` seals
  all source/config files used by the final gate.
- `context.json` identifies platform, toolchain and preserved design assets.
- `sha256.json` hashes every retained evidence file except itself.

This preparation work changes no painted interface and makes no new GUI,
aesthetic, keyboard, acoustic or performance acceptance claim. The existing
UI harness remains included. Core schema 28 and database schema 34 are unchanged; no DP or gate is
promoted. Git metadata was read-only, so there is no new commit or push.
''')
(e/'verification.json').write_text(json.dumps({'checks':checks,'no_gui_change':True,'no_new_gui_or_device_claim':True,'no_requirement_or_gate_promoted':True},indent=2)+'\n')
subprocess.run(['python3',str(s/'summarize-tree.py')],check=True)
checked=subprocess.run(['git','-c','core.fsmonitor=false','diff','--check'],cwd=r,capture_output=True,text=True)
(s/'whitespace.json').write_text(json.dumps({'exit_code':checked.returncode,'stdout':checked.stdout,'stderr':checked.stderr},indent=2)+'\n')
checked.check_returncode()
for name in ['context.json','review.json','final-source.json','doc-links.json','increment.json','whitespace.json','gate.py','scope.py','summarize-tree.py','collect.py','checkpoint.py','retry-playback.py','diagnose-playback.py','serial-playback.py']:
    shutil.copyfile(s/name,e/name)
(e/'increment.diff.gz').write_bytes(gzip.compress((s/'increment.diff').read_bytes(),mtime=0))
hashes={str(p.relative_to(e)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(e.rglob('*')) if p.is_file() and p!=e/'sha256.json'}
(e/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
subprocess.run(['python3',str(s/'checkpoint.py')],check=True)
print(json.dumps({'evidence_files_verified':len(hashes),'source_files':len(source)}))
