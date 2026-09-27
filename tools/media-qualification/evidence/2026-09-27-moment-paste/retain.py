import gzip
import hashlib
import json
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-moment-paste-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-27-moment-paste'
out.mkdir(exist_ok=False)
gate = json.loads((scratch / 'gate-3/report.json').read_text())
source = json.loads((scratch / 'gate-3/source-after.json').read_text())
review = json.loads((scratch / 'review.json').read_text())
assert gate['source_unchanged']
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in source.items())
assert all(source[p] == h for p, h in review['source_sha256'].items())
files = [p.name for p in scratch.iterdir() if p.is_file() and p.suffix in ('.py', '.json', '.log', '.diff', '.patch', '.tsv')]
for directory in ['gate-1', 'gate-2', 'gate-3', 'remaining', 'final-ui', 'ui-moment', 'ui-moment-performance', 'ui-moment-final-visual', 'ui-moment-final-performance', 'store']:
    files.extend(str(p.relative_to(scratch)) for p in (scratch / directory).rglob('*') if p.is_file())
for name in sorted(set(files)):
    path = scratch / name
    compressed = path.suffix in ('.log', '.diff', '.patch')
    target = out / (name + '.gz' if compressed else name)
    target.parent.mkdir(parents=True, exist_ok=True)
    data = path.read_bytes()
    target.write_bytes(gzip.compress(data, mtime=0) if compressed else data)
summary = json.loads((scratch / 'coverage.json').read_text())
summary.update(document_schema=27, database_schema=33, reviewed_source_paths=len(review['source_sha256']), no_requirement_or_gate_promoted=True, other_agent_harness_preserved=True)
(out / 'verification.json').write_text(json.dumps(summary, indent=2) + '\n')
(out / 'README.md').write_text('''# Original moment paste evidence

See [qualification](../../../../docs/qualification/moment-paste-2026-09-27.md)
for scope, results and limits. Logs and incremental diffs are compressed.
The increment compares the preceding verified checkpoint, not Git HEAD.
`gate-3` checks the final unchanged source; `remaining` covers targets after
the sandbox-denied socket test and the first two UI replay attempts.
`final-ui` repeats visual and release replay after the data-only Kestrel digest
refresh. `gate-1` retains the earlier run and its two reviewed UI corrections;
`gate-2` predates the reviewed shortcut fixture refresh. Non-app source is
unchanged between the remaining-target tests and the final gate.

The contributed harness, ten ImageGen boards and prompts, and all prior pending
work are preserved. No requirement or product gate is promoted. Git metadata
is read-only in this session, so no commit or push is claimed.
''')
hashes = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')
print(json.dumps({'files': len(hashes), 'evidence': str(out)}))
