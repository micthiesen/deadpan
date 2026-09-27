from pathlib import Path
import hashlib
import json
import shutil
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-retime-20260927')
prior = Path('/tmp/deadpan-original-audition-20260927')
expected = json.loads((prior / 'gate-2/source-after.json').read_text())
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in expected.items())
paths = set(expected) | {'AGENTS.md', 'README.md'}
paths.update(str(p.relative_to(repo)) for p in (repo / 'docs').rglob('*.md'))
for name in sorted(paths):
    target = scratch / 'before' / name
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(repo / name, target)
binary = scratch / 'old-deadpan-cli'
shutil.copy2(repo / 'target/debug/deadpan-cli', binary)
head = subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo).decode().strip()
(scratch / 'baseline.json').write_text(json.dumps(dict(
    base_revision=head, previous_checkpoint=str(prior / 'checkpoint'),
    source_sha256=expected, old_binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
), indent=2) + '\n')
print(json.dumps(dict(source_count=len(expected), copied_files=len(paths), old_binary=str(binary))))
