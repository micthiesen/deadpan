import gzip, hashlib, json
from pathlib import Path
repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-interior-splice-20260926')
out=repo/'tools/media-qualification/evidence/2026-09-26-interior-insertion'
out.mkdir(exist_ok=False)
files=['baseline.json','context.json','review.json','old-binary.json','migration-source-manifest.json','increment.patch','increment-paths.json','core-initial.log','core.log','audio.log','migration.log','app-checks.json','app-check-0.log','app-check-1.log','app-check-2.log','gate.py','post-validation.py','durable-cli.py','run-app-checks.py','old-context-refusal.py','increment.py','retain.py']
for folder in ('gate-1','post-validation','ui-editing','durable-app','old-context-refusal','old-context-refusal-02'):
    for path in sorted((scratch/folder).rglob('*')):
        if path.is_file() and path.suffix in ('.log','.json','.py'):
            files.append(str(path.relative_to(scratch)))
for name in files:
    source=scratch/name
    data=source.read_bytes()
    compress=source.suffix=='.log' or source.name=='commands.json' or (source.name=='report.json' and source.parent.name=='ui-editing')
    target=out/(name+'.gz' if compress else name)
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_bytes(gzip.compress(data,mtime=0) if compress else data)
gate=json.loads((scratch/'gate-1/report.json').read_text())
post=json.loads((scratch/'post-validation/report.json').read_text())
expected=json.loads((scratch/'post-validation/source-after.json').read_text())
assert gate['source_unchanged']
assert post['source_unchanged']
assert all(hashlib.sha256((repo/p).read_bytes()).hexdigest()==h for p,h in expected.items())
provenance=json.loads((repo/'crates/deadpan-store/tests/fixtures/v30-interior-insert-history.provenance.json').read_text())
fixture=repo/'crates/deadpan-store/tests/fixtures'
for name,key in [('v30-interior-insert-history.sql','sql_sha256'),('produce-v30-interior-insert-history.py','producer_script_sha256'),('v30-interior-insert-history.commands.json','producer_log_sha256')]:
    assert hashlib.sha256((fixture/name).read_bytes()).hexdigest()==provenance[key]
summary=dict(gate=gate,post_validation=post,app_checks=json.loads((scratch/'app-checks.json').read_text()),durable_headless=json.loads((scratch/'durable-app/report.json').read_text()),old_binary_refusal=json.loads((scratch/'old-context-refusal-02/summary.json').read_text()),fixture_provenance_verified=True,source_count=len(expected),sources_match_final_gate=True)
(out/'verification.json').write_text(json.dumps(summary,indent=2)+'\n')
(out/'README.md').write_text('# Interior pause insertion evidence\n\nSee [qualification](../../../../docs/qualification/interior-insertion-2026-09-26.md) for scope, reviews, exact checks and remaining work. Logs and large command/replay reports are gzip-compressed. `increment.patch` is against the preceding source checkpoint, not Git HEAD. The fixture producer, authentic DB30 SQL and provenance are kept in `crates/deadpan-store/tests/fixtures`. No file here establishes a new visual, device, performance or full-product result.\n')
hashes={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
(out/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(json.dumps(dict(retained_files=len(hashes),evidence=str(out))))
