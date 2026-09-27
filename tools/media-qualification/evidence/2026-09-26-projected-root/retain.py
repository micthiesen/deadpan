import gzip
import hashlib
import json
import re
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-root-projection-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-projected-root'
out.mkdir(exist_ok=False)
files = ['baseline.json', 'review.json', 'next-step.md', 'increment.patch', 'increment-paths.json',
         'gate.py', 'increment.py', 'retain.py', 'qualification.py', 'snapshot.py']
files += [f'focused-{suite}-{index}.log' for suite in ('audio', 'plan') for index in (1, 2)]
files += [str(p.relative_to(scratch)) for p in sorted((scratch / 'gate-1').glob('*')) if p.is_file()]
for name in files:
    source = scratch / name
    compress = source.suffix == '.log'
    target = out / (name + '.gz' if compress else name)
    target.parent.mkdir(parents=True, exist_ok=True)
    data = source.read_bytes()
    target.write_bytes(gzip.compress(data, mtime=0) if compress else data)
expected = json.loads((scratch / 'gate-1/source-after.json').read_text())
gate = json.loads((scratch / 'gate-1/report.json').read_text())
assert gate['source_unchanged']
focused = []
for suite in ('audio', 'plan'):
    focused += re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',
                          (scratch / f'focused-{suite}-2.log').read_text())
assert sum(int(row[0]) for row in focused) == 62
assert all(row[1:] == ('0', '0') for row in focused)
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in expected.items())
reviewed = json.loads((scratch / 'review.json').read_text())['source_sha256']
assert all(expected[p] == h for p, h in reviewed.items())
summary = dict(gate=gate, source_count=len(expected), sources_match_final_gate=True,
    independent_review_matches_final_gate=True, focused_tests=62,
    new_plan_tests=3, new_pcm_tests=3, extended_decay_test=True,
    document_schema=25, database_schema=31, authored_schema_unchanged=True,
    gui_replay_run=False,
    gui_reason='No UI, input, lifecycle or shader change. Absolute projected output is tested with decoded PCM and the canonical DSP path; earlier GUI findings remain open.')
(out / 'verification.json').write_text(json.dumps(summary, indent=2) + '\n')
(out / 'README.md').write_text('''# Projected root audio evidence

See [qualification](../../../../docs/qualification/projected-root-2026-09-26.md)
for scope, independent review, actual PCM checks and remaining splice work.
Logs are compressed. `increment.patch` is against the preceding checkpoint,
not Git HEAD. Borrowed root placement does not author routes or change normal
root-plan evaluation. Compiler integration, GUI, accessibility and release
qualification remain open. The UI harness and ImageGen targets are preserved.
''')
hashes = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')
print(json.dumps(dict(retained_files=len(hashes), evidence=str(out))))
