import difflib, hashlib, json
from pathlib import Path
repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-splice-representation-20260926')
baseline = json.loads((scratch / 'baseline.json').read_text())['source_hashes']
paths = set(baseline)
for root in ('crates', 'native'):
    for p in (repo / root).rglob('*'):
        if p.is_file() and p.suffix in ('.rs', '.toml', '.cpp', '.h', '.c', '.py', '.sql', '.json'):
            paths.add(str(p.relative_to(repo)))
changed = []
parts = []
for name in sorted(paths):
    current = repo / name
    if not current.is_file():
        continue
    content = current.read_bytes()
    if name in baseline and hashlib.sha256(content).hexdigest() == baseline[name]:
        continue
    old = scratch / 'before' / name
    if name not in baseline and old.exists():
        continue
    changed.append(name)
    parts.extend(difflib.unified_diff(old.read_text().splitlines(True) if old.exists() else [], content.decode().splitlines(True), fromfile='before/' + name, tofile=name))
(scratch / 'increment.patch').write_text(''.join(parts))
(scratch / 'increment-paths.json').write_text(json.dumps(changed, indent=2)+'\n')
print(json.dumps(changed, indent=2))
