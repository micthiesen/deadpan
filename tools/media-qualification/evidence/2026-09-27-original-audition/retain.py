"""Retain exact verification, review and source identity without modifying Git."""
from pathlib import Path
import gzip,hashlib,json,re,subprocess
r=Path('/Users/michael/Code/deadpan');s=Path('/tmp/deadpan-original-audition-20260927')
out=r/'tools/media-qualification/evidence/2026-09-27-original-audition'
source=json.loads((s/'stable-before-tests.json').read_text())['source_sha256']
assert all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in source.items())
review=json.loads((s/'review.json').read_text())
assert all(source[p]==h for p,h in review['source_sha256'].items())
gate=json.loads((s/'gate-2/report.json').read_text())
final=json.loads((s/'final-checks/report.json').read_text())
assert final['sources_match_pretest_seal']
assert len(gate['commands'])==7
assert all(row['exit_code']==0 for i,row in enumerate(gate['commands']) if i!=2)
assert len(final['commands'])==3
assert json.loads((s/'gate-2/source-after.json').read_text())==source
assert all(c['exit_code']==0 for c in final['commands'][:2])
out.mkdir(exist_ok=False)
files=[p for p in s.iterdir() if p.is_file() and p.suffix in ('.json','.py','.log','.patch','.diff')]
for name in ['backend','gate-1','gate-2','final-checks','ui-original-playback']:
 files.extend(p for p in (s/name).rglob('*') if p.is_file())
for p in sorted(files):
 rel=p.relative_to(s);compressed=p.suffix in ('.log','.patch','.diff')
 target=out/(str(rel)+('.gz' if compressed else ''));target.parent.mkdir(parents=True,exist_ok=True)
 target.write_bytes(gzip.compress(p.read_bytes(),mtime=0) if compressed else p.read_bytes())
summary={'source_count':len(source),'reviewed_source_paths':len(review['source_sha256']),'stable_before_workspace_tests':True,
 'document_schema':27,'database_schema':33,'gate':gate,'final_checks':final,
 'no_requirement_or_gate_promoted':True,'other_agent_harness_preserved':True,
 'review':'Three independent static reviews; one measured-selection endpoint finding fixed and re-reviewed.',
 'git':'Metadata read-only; no commit/push claimed.'}
(out/'verification.json').write_text(json.dumps(summary,indent=2)+'\n')
(out/'README.md').write_text('''# Original and selection audition evidence

See [qualification](../../../../docs/qualification/original-audition-2026-09-27.md).
Logs and the incremental source patch are compressed. The increment compares the
preceding verified checkpoint, not Git HEAD. All prior pending work is retained.

The initial gate stopped at formatting. Review changes landed during the second
gate's Clippy invocation; stable-before-tests.json seals the final source before
workspace tests started. All remaining checks used that unchanged source.
Final checks repeat formatting and workspace lint after the review correction,
then attempt the contributed production GUI replay. Source identity is verified
against both the pre-test seal and independent review snapshot.

The harness injects delivery updates; separate backend tests consume actual
canonical PCM through a controlled device queue. Neither establishes listening,
physical display or native accessibility qualification. ImageGen targets and
prompts remain intact. Git metadata is read-only, so no commit or push is claimed.
''')
hashes={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
(out/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(json.dumps({'evidence':str(out),'files':len(hashes)}))
