"""Retain reviewed source identity and actual verification, without Git writes."""
from pathlib import Path
import gzip,hashlib,json
r=Path('/Users/michael/Code/deadpan'); s=Path('/tmp/deadpan-retime-20260927')
out=r/'tools/media-qualification/evidence/2026-09-27-retime-editing'
pretest=json.loads((s/'gate-1/source-before-tests.json').read_text())
source=json.loads((s/'final-source.json').read_text())
assert all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in source.items())
assert json.loads((s/'gate-1/source-after.json').read_text())==pretest
assert {p for p in source if source[p]!=pretest[p]}=={'crates/deadpan-cli/src/doctor.rs','crates/deadpan-cli/tests/project_commands.rs'}
review=json.loads((s/'review.json').read_text())
assert all(source[p]==h for p,h in review['source_sha256'].items())
gate=json.loads((s/'gate-1/report.json').read_text())
final=json.loads((s/'final-checks/report.json').read_text())
assert len(gate['commands'])==7
assert all(c['exit_code']==0 for i,c in enumerate(gate['commands']) if i!=2)
assert gate['commands'][2]['tests']=={'passed':1700,'failed':2,'ignored':0}
assert len(final['commands'])==5 and final['sources_match_final_seal']
assert all(c['exit_code']==0 for c in final['commands'][:4])
assert final['commands'][2]['tests']=={'passed':17,'failed':0,'ignored':0}
out.mkdir(exist_ok=False)
files=[p for p in s.iterdir() if p.is_file() and p.suffix in ('.json','.py','.log','.patch','.diff')]
for name in ['core','history','gate-1','final-checks','ui-retime']:
 files.extend(p for p in (s/name).rglob('*') if p.is_file() and p.suffix in ('.json','.py','.log','.patch','.diff','.txt','.png','.html'))
for p in sorted(files):
 rel=p.relative_to(s); compressed=p.suffix in ('.log','.patch','.diff')
 target=out/(str(rel)+('.gz' if compressed else '')); target.parent.mkdir(parents=True,exist_ok=True)
 target.write_bytes(gzip.compress(p.read_bytes(),mtime=0) if compressed else p.read_bytes())
summary={
 'source_count':len(source),'reviewed_source_paths':len(review['source_sha256']),
 'stable_during_workspace_tests':True,'document_schema':28,'database_schema':34,
 'post_gate_changes':'CLI doctor labels and schema-version test corrected; all 17 CLI command tests rerun successfully.',
 'gate':gate,'final_checks':final,'no_requirement_or_gate_promoted':True,
 'other_agent_harness_preserved':True,
 'review':'Three independent static reviews; one processing-history PCM oracle finding addressed and re-reviewed.',
 'git':'Metadata read-only; no commit/push claimed.'
}
(out/'verification.json').write_text(json.dumps(summary,indent=2)+'\n')
(out/'README.md').write_text('''# Structural speed editing evidence

See [qualification](../../../../docs/qualification/retime-editing-2026-09-27.md).
Logs and the incremental source patch are compressed. The increment compares the
preceding verified Original audition checkpoint, not Git HEAD. Prior pending
work, including the contributed UI harness, is retained.

The final PCM oracle correction landed during the first workspace Clippy run.
`gate-1/source-before-tests.json` seals all source/configuration before workspace
tests started. Remaining gate steps use that unchanged source. The run exposed
a stale CLI schema assertion alongside the known sandbox socket-bind failure.
After the gate, only the CLI doctor labels and its schema test changed. Final
checks repeat formatting, workspace lint, all 17 CLI command tests and doctor,
then attempt production GUI replay. `final-source.json` records that final
source; its two changes from the gate seal and independent review hashes are
checked explicitly. The workspace run itself is not reported as all passing.

The GUI replay requires Metal. Its report records actual startup, executed
steps and captures; the scenario's existence is not a claim that it ran.
Independent audio tests decode actual PCM, use canonical DSP references and
check retained processing history. No physical display, native accessibility,
listening, long-input Preserve or encoded export qualification is claimed.
ImageGen targets and prompts remain intact. Git metadata is read-only, so no
commit or push is claimed.
''')
hashes={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
(out/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(json.dumps({'evidence':str(out),'files':len(hashes)}))
