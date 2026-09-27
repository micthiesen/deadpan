"""Adapt the preceding read-only checkpoint audit to the final tested tree."""
from pathlib import Path
import hashlib
import json
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-original-audition-20260927')
previous = Path('/tmp/deadpan-moment-paste-20260926')
evidence = repo / 'tools/media-qualification/evidence/2026-09-27-original-audition'

audit = (previous / 'summarize-tree.py').read_text()
assert audit.count(str(previous)) == 1
audit = audit.replace(str(previous), str(scratch))
(scratch / 'summarize-tree.py').write_text(audit)
subprocess.run(['python3', str(scratch / 'summarize-tree.py')], check=True)
checked = subprocess.run(
    ['git', '-c', 'core.fsmonitor=false', 'diff', '--check'],
    cwd=repo, capture_output=True, text=True,
)
(scratch / 'whitespace.json').write_text(json.dumps({
    'exit_code': checked.returncode,
    'stdout': checked.stdout,
    'stderr': checked.stderr,
}, indent=2) + '\n')
checked.check_returncode()

for name in ['summarize-tree.py', 'doc-links.json', 'increment.json', 'whitespace.json', 'finalize-evidence.py']:
    (evidence / name).write_bytes((scratch / name).read_bytes())
hashes = {
    str(p.relative_to(evidence)): hashlib.sha256(p.read_bytes()).hexdigest()
    for p in sorted(evidence.rglob('*'))
    if p.is_file() and p != evidence / 'sha256.json'
}
(evidence / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')

checkpoint = (previous / 'checkpoint.py').read_text()
replacements = {
    str(previous): str(scratch),
    'gate-3/source-after.json': 'gate-2/source-after.json',
    'evidence/2026-09-27-moment-paste': 'evidence/2026-09-27-original-audition',
    '/tmp/deadpan-group-navigation-20260926/checkpoint': str(previous / 'checkpoint'),
}
for old, new in replacements.items():
    assert checkpoint.count(old) == 1, old
    checkpoint = checkpoint.replace(old, new)
(scratch / 'checkpoint.py').write_text(checkpoint)
subprocess.run(['python3', str(scratch / 'checkpoint.py')], check=True)
