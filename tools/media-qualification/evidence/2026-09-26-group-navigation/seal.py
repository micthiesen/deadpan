import hashlib, json, shutil
from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
out=repo/'tools/media-qualification/evidence/2026-09-26-group-navigation'
for name in ['check-doc-links.py','doc-links.json','seal.py']:
    shutil.copy2(scratch/name,out/name)
assert not json.loads((out/'doc-links.json').read_text())['missing']
source=json.loads((out/'gate-2/source-after.json').read_text())
assert all(hashlib.sha256((repo/p).read_bytes()).hexdigest()==h for p,h in source.items())
hashes={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(out.rglob('*')) if p.is_file() and p!=out/'sha256.json'}
(out/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(json.dumps(dict(files=len(hashes),source_files=len(source),source_unchanged=True)))
