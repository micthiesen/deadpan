import gzip
import hashlib
import json
import re
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-audio-projection-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-audio-input-tapes'
out.mkdir(exist_ok=False)
files = ['baseline.json', 'review.json', 'increment.patch', 'increment-paths.json',
         'gate.py', 'increment.py', 'review.py', 'retain.py', 'qualification.py',
         'check-2.log', 'focused-1.log', 'focused-2.log']
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
focused = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',
                     (scratch / 'focused-2.log').read_text())
assert sum(int(row[0]) for row in focused) == 44
assert all(row[1:] == ('0', '0') for row in focused)
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in expected.items())
reviewed = json.loads((scratch / 'review.json').read_text())['source_sha256']
assert all(expected[p] == h for p, h in reviewed.items())
summary = dict(gate=gate, source_count=len(expected), sources_match_final_gate=True,
    independent_review_matches_final_gate=True, focused_tests=44,
    new_plan_tests=7, new_pcm_tests=8, document_schema=25, database_schema=31,
    authored_schema_unchanged=True, gui_replay_run=False,
    gui_reason='No UI, input, lifecycle or shader change. Current tape preparation is tested with actual decoded PCM and the shared canonical DSP path. Existing GUI findings remain open.')
(out / 'verification.json').write_text(json.dumps(summary, indent=2) + '\n')
(out / 'README.md').write_text('''# Live audio input tape evidence

See [qualification](../../../../docs/qualification/audio-input-tapes-2026-09-26.md)
for scope, independent review, actual PCM checks and remaining splice work.
Logs are compressed. `increment.patch` is against the preceding source
checkpoint, not Git HEAD. No authored splice route or changed intrinsic
Preserve stage is implemented here. GUI, native accessibility and release
qualification remain open; the existing UI harness and ImageGen targets remain.
''')
hashes = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')
print(json.dumps(dict(retained_files=len(hashes), evidence=str(out))))
