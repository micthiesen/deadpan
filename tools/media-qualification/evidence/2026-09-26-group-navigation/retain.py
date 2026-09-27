import gzip, hashlib, json
from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
out=repo/'tools/media-qualification/evidence/2026-09-26-group-navigation'
out.mkdir(exist_ok=False)
gate=json.loads((scratch/'gate-2/report.json').read_text())
source=json.loads((scratch/'gate-2/source-after.json').read_text())
review=json.loads((scratch/'review.json').read_text())
assert gate['source_unchanged']
assert all(hashlib.sha256((repo/p).read_bytes()).hexdigest()==h for p,h in source.items())
assert all(source[p]==h for p,h in review['source_sha256'].items())
files=[p.name for p in scratch.iterdir() if p.is_file() and p.suffix in ('.py','.json','.log','.diff','.patch')]
for directory in ['gate-1','gate-2','ui-nested']:
    files.extend(str(p.relative_to(scratch)) for p in (scratch/directory).rglob('*') if p.is_file())
for name in sorted(set(files)):
    path=scratch/name
    compressed=path.suffix in ('.log','.diff','.patch')
    target=out/(name+'.gz' if compressed else name)
    target.parent.mkdir(parents=True,exist_ok=True)
    data=path.read_bytes()
    target.write_bytes(gzip.compress(data,mtime=0) if compressed else data)
summary=json.loads((scratch/'coverage.json').read_text())
summary.update(document_schema=26,database_schema=32,reviewed_source_paths=len(review['source_sha256']),
    no_requirement_or_gate_promoted=True,other_agent_harness_preserved=True)
(out/'verification.json').write_text(json.dumps(summary,indent=2)+'\n')
(out/'README.md').write_text('''# Sequence navigation evidence

See [qualification](../../../../docs/qualification/group-navigation-2026-09-26.md)
for scope, checks, review and limits. Logs and diffs are compressed. The increment
diff compares the preceding verified checkpoint, not Git HEAD. `gate-2` is final;
`gate-1` retains the broader storage checks before the four preview-only review
changes. Source manifests identify both states. The UI attempt precedes those
corrections and failed before app construction; it is not a visual pass.

The contributed harness and all prior implementation/design work are preserved.
The full project goal remains active and Git writes are unavailable here.
''')
hashes={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
(out/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(json.dumps(dict(files=len(hashes),evidence=str(out))))
