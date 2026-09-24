import gzip, hashlib, json, re, shutil, subprocess
from pathlib import Path
r=Path('/Users/michael/Code/deadpan')
s=Path('/tmp/deadpan-master-20260924')
d=r/'tools/audio-limiter-qualification/evidence/2026-09-24-limited-app'
d.mkdir(parents=True,exist_ok=False)
files=[]
for name in ('final-1','final-2','final-3'):
    files += [(p,Path('gate')/name/p.name) for p in (s/'gate'/name).iterdir() if p.is_file()]
for name in ('gate.py','design-spec-verification.json'):
    files.append((s/name,Path(name)))
for name in ('build.log','build.json','environment.json','source-hashes.json','project-before.json','project-after.json','native-review.json'):
    files.append((s/'native-gui'/name,Path('native')/name))
for source, relative in files:
    target=d/relative
    if source.suffix=='.log':
        target=target.with_suffix('.log.gz'); payload=gzip.compress(source.read_bytes(),mtime=0)
    else: payload=source.read_bytes()
    target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(payload)
report=json.loads((s/'gate/final-3/report.json').read_text())
assert len(report['commands'])==5 and all(x['exit_code']==0 for x in report['commands'])
before=json.loads((s/'gate/final-3/source-before.json').read_text())
after=json.loads((s/'gate/final-3/source-after.json').read_text())
changed=[p for p in before if before[p]!=after[p]]
assert changed==['crates/deadpan-playback/README.md'],changed
current=(r/changed[0]).read_bytes()
old=current.replace(b'at most 8192 frames',b'at most8192frames')
assert hashlib.sha256(old).hexdigest()==before[changed[0]]
assert hashlib.sha256(current).hexdigest()==after[changed[0]]
assert all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in after.items())
native=json.loads((s/'native-gui/build.json').read_text())
assert native['exit_code']==0 and native['source_unchanged'] and native['binary_sha256']==native['bundle_sha256']
a=json.loads((s/'native-gui/project-before.json').read_text()); b=json.loads((s/'native-gui/project-after.json').read_text())
assert a['database_dump_sha256']==b['database_dump_sha256']
summary={'required_commands_passed':True,'tests':report['tests'],'code_and_fixture_paths_unchanged':len(after)-1,'documentation_change_during_gate':{'path':changed[0],'change':'Added two missing spaces to at most 8192 frames. The raw gate runner includes crate Markdown in its source seal and therefore reports source_unchanged=false and passed=false. All five required commands passed; this exact byte substitution is independently checked here.'},'optimized_native_build':native,'project_dump_unchanged':True,'prior_failures':'final-1 and final-2 retain canonical-reference ten-second deadline failures before matching the existing sixty-second production budget. The production deadline was not increased. final-1/count-correction.json corrects its original parser omission of a failed test binary.'}
(d/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
shutil.copy2(__file__,d/'retain.py')
manifest={str(p.relative_to(d)): {'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(d.rglob('*')) if p.is_file()}
(d/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(summary,indent=2))
