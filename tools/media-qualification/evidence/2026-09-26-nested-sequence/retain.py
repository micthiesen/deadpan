import gzip
import hashlib
import json
import re
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-nested-sequence-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-nested-sequence'
out.mkdir(exist_ok=False)
gate = json.loads((scratch / 'gate-1/report.json').read_text())
expected = json.loads((scratch / 'gate-1/source-after.json').read_text())
assert gate['source_unchanged']
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in expected.items())
review = json.loads((scratch / 'review.json').read_text())
assert all(expected[p] == h for p, h in review['source_sha256'].items())
files = ['baseline.json', 'review.json', 'review-source.json', 'next-step.md',
         'increment.patch', 'increment-paths.json', 'gate.py', 'retain.py', 'qualify.py',
         'core25-binary.json', 'probe-core25-context.py', 'run-gui.py', 'gui-exit.json']
files += [p.name for p in scratch.glob('*.log')]
files += [str(p.relative_to(scratch)) for p in sorted((scratch/'old-context-refusal').glob('*.json'))]
for directory in ['gate-1', 'gui']:
    files += [str(p.relative_to(scratch)) for p in sorted((scratch / directory).rglob('*')) if p.is_file()]
for name in sorted(set(files)):
    source = scratch / name
    compress = source.suffix in ('.log', '.patch')
    target = out / (name + '.gz' if compress else name)
    target.parent.mkdir(parents=True, exist_ok=True)
    data = source.read_bytes()
    target.write_bytes(gzip.compress(data, mtime=0) if compress else data)
focused = {}
for name in ['core-1', 'audio-1', 'migration-1', 'cli-2', 'native-1']:
    rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',
                      (scratch / f'focused-{name}.log').read_text())
    focused[name] = dict(zip(('passed', 'failed', 'ignored'),
                            (sum(int(row[i]) for row in rows) for i in range(3))))
summary = dict(gate=gate, focused=focused, source_count=len(expected),
    reviewed_increment_paths=len(review['source_sha256']),
    sources_match_final_gate=True, review_hashes_match=True,
    document_schema=26, database_schema=32,
    gui=json.loads((scratch/'gui-exit.json').read_text()),
    scope='Actual pause insertion under unretimed Sequence ancestors; no fractional clocks, Repeat/Retime ancestor insertion or nested native inspector',
    other_agent_harness_preserved=True, no_requirement_or_gate_promoted=True)
(out/'verification.json').write_text(json.dumps(summary, indent=2)+'\n')
(out/'README.md').write_text('''# Nested Sequence insertion evidence

See [qualification](../../../../docs/qualification/nested-sequence-2026-09-26.md)
for command, native, PCM, CLI and migration scope, review, failures and limits.
Logs and the source increment patch are compressed. That patch is against the
preceding verified checkpoint, not Git HEAD. `gui-exit.json` and `gui/report.json`
distinguish actual keyboard assertions from startup failure and shortcut audit.
The full goal remains active; the concurrent harness and design boards survive.
''')
hashes = {str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest()
          for p in sorted(out.rglob('*')) if p.is_file()}
(out/'sha256.json').write_text(json.dumps(hashes, indent=2)+'\n')
print(json.dumps(dict(retained_files=len(hashes), evidence=str(out))))
