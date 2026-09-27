import gzip, hashlib, json
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-boundary-routing-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-boundary-location'
out.mkdir(exist_ok=False)
files = ['baseline.json', 'review.json', 'increment.patch', 'increment-paths.json',
    'old-binary.json', 'gate.py', 'headless.py', 'increment.py', 'retain.py']
for folder in ('gate-1', 'headless'):
    for path in sorted((scratch / folder).rglob('*')):
        if path.is_file() and path.suffix in ('.log', '.json'):
            files.append(str(path.relative_to(scratch)))
for name in files:
    source = scratch / name
    data = source.read_bytes()
    compress = source.suffix == '.log' or source.name == 'commands.json'
    target = out / (name + '.gz' if compress else name)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(gzip.compress(data, mtime=0) if compress else data)
gate = json.loads((scratch / 'gate-1/report.json').read_text())
expected = json.loads((scratch / 'gate-1/source-after.json').read_text())
assert gate['source_unchanged']
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in expected.items())
summary = dict(gate=gate, headless=json.loads((scratch / 'headless/report.json').read_text()),
    source_count=len(expected), sources_match_final_gate=True,
    document_schema=25, database_schema=31, authored_schema_unchanged=True)
(out / 'verification.json').write_text(json.dumps(summary, indent=2) + '\n')
(out / 'README.md').write_text('''# Exact boundary location evidence

See [qualification](../../../../docs/qualification/boundary-location-2026-09-26.md)
for scope, independent reviews, checks and remaining work. Logs and command
records are gzip-compressed. `increment.patch` is against the preceding source
checkpoint, not Git HEAD. All query fixtures use synthetic Background/Silence
structure. No new GUI, device, media, latency or full-product qualification is
claimed. Existing UI harness sources and ImageGen targets remain in the project.
''')
hashes = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
    for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')
print(json.dumps(dict(retained_files=len(hashes), evidence=str(out))))
