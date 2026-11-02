from pathlib import Path
import hashlib
import json
import sys

home = Path(__file__).resolve().parent
evidence = Path('/tmp/deadpan-stage-block-policy-admission-2026-11-01')
audit_path = Path(sys.argv[1])
def pin(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        while block := stream.read(1048576):
            digest.update(block)
    return {'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}

audit = json.loads(audit_path.read_bytes())
assert audit['verdict'] == 'GO_BOUNDED_ARCHIVE_REFERENCE_AND_CURATED_SOURCE'
assert audit['findings'] == []
inventory_pin = pin(home / 'archive/inventory.json')
assert {key: audit['inventory'][key] for key in inventory_pin} == inventory_pin
inventory = json.loads((home / 'archive/inventory.json').read_bytes())
rows = {row['path']: row for row in inventory['records']}
docs = json.loads((evidence / 'documentation-pins.json').read_bytes())
fixtures = {f'crates/deadpan-audio/tests/fixtures/stage-block-policy-baseline-v1/{name}': None for name in ['corpus.json', 'README.md']}
for mapping in [docs, fixtures]:
    for name in mapping:
        row = rows['shipping-source/' + name]
        expected = {key: row[key] for key in ['bytes', 'sha256']}
        if mapping is docs:
            assert mapping[name] == expected
        mapping[name] = expected
        for root in [Path('/Users/michael/Code/deadpan'), Path('/tmp/deadpan-query-groundwork-2026-10-26/scoped')]:
            assert pin(root / name) == expected, name
reports = {
    'independent-archive-audit.json': audit_path,
    'independent-archive-auditor.py': audit_path.with_name('audit_archive.py'),
    'independent-runtime-review.json': Path('/tmp/deadpan-stage-block-policy-runtime-acceptance-review-2026-11-01/review.json'),
    'independent-documentation-review.json': Path('/tmp/deadpan-stage-block-policy-documentation-review-2026-11-01/v3-final-runtime/review.json'),
    'independent-input-review.json': Path('/tmp/deadpan-stage-block-policy-final-input-review-2026-11-01/final-input-review.json'),
    'packaging-execution-proof.json': home / 'packaging-execution-proof.json',
    'prepare-archive-gate.py': Path(__file__),
}
reports.update({'packaging-' + name: home / name for name in ['freeze.stdout', 'freeze.stderr', 'build.stdout', 'build.stderr']})
out = home / 'final-archive-gate.json'
assert not out.exists()
out.write_text(json.dumps({'schema': 'O-final-archive-docs-source-gate-v1',
                          'verdict': 'GO_ARCHIVE_DOCS_SOURCE', 'inventory': inventory_pin,
                          'documentation': docs, 'fixtures': fixtures,
                          'reports': [{'source': str(path), 'destination': name, **pin(path)} for name, path in reports.items()],
                          'scope': 'Qualified O source, runtime evidence, documentation and independently verified archive only. Actual mirror, curated Git staging/commit and remote publication require separate proofs. No product Gate completion.'}, indent=2) + '\n')
print(json.dumps({'gate': str(out), **pin(out)}))
