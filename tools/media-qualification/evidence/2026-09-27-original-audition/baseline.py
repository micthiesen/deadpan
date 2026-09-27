import hashlib
import json
from pathlib import Path
import shutil
import subprocess

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-original-audition-20260927')
prior=Path('/tmp/deadpan-moment-paste-20260926')
expected=json.loads((prior/'gate-3/source-after.json').read_text())
assert all(hashlib.sha256((repo/p).read_bytes()).hexdigest()==h for p,h in expected.items())
paths=set(expected)|{'AGENTS.md','README.md'}
paths.update(str(p.relative_to(repo)) for p in (repo/'docs').rglob('*.md'))
for name in sorted(paths):
    target=scratch/'before'/name
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(repo/name,target)
assert all(hashlib.sha256((scratch/'before'/p).read_bytes()).hexdigest()==h for p,h in expected.items())
head=subprocess.check_output(['git','-c','core.fsmonitor=false','rev-parse','HEAD'],cwd=repo).decode().strip()
(scratch/'baseline.json').write_text(json.dumps(dict(base_revision=head,previous_checkpoint=str(prior/'checkpoint'),source_sha256=expected),indent=2)+'\n')
print(json.dumps(dict(source_count=len(expected),copied_files=len(paths),source_matches_previous_gate=True)))
