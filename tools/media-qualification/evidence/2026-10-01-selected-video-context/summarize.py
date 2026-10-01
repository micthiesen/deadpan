import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
pattern = re.compile(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out')

def counts(text):
    rows = [tuple(map(int, match.groups())) for match in pattern.finditer(text)]
    return {'targets': len(rows), **dict(zip(('passed', 'failed', 'ignored', 'measured', 'filtered'), map(sum, zip(*rows))))} if rows else {'targets': 0}

reports = {}
for path in sorted(root.glob('*.json')):
    if path.name.startswith('source-'):
        continue
    report = json.loads(path.read_text())
    if 'command' not in report:
        continue
    log = path.with_suffix('.log').read_text()
    report['tests'] = counts(log)
    report['unit_and_integration'] = counts(log.split('Doc-tests ')[0])
    report['doc_tests'] = counts(log[log.index('Doc-tests '):]) if 'Doc-tests ' in log else {'targets': 0}
    reports[path.stem] = report

broad = (root / 'workspace-ui-tests.log').read_text()
marker = 'Running unittests src/lib.rs (target/debug/deps/deadpan_store-'
assert broad.count(marker) == 1
reports['verified_nonstore_prefix'] = counts(broad.split(marker)[0])
if 'storage-tests' in reports and reports['storage-tests'].get('exit_code') == 0:
    reports['distinct_workspace_tests'] = {
        key: reports['verified_nonstore_prefix'][key] + reports['storage-tests']['unit_and_integration'][key]
        for key in ('targets', 'passed', 'failed', 'ignored')
    }

baseline = json.loads((root / 'source-46f37ba08f700b88acef80e2670d169d52fb89f0a52df192a2a04d7bfd7e1e5f.json').read_text())
for label in ('formatting-final', 'storage-tests', 'workspace-doc-tests', 'strict-clippy', 'formatting-complete', 'strict-clippy-reviewed', 'formatting-reviewed', 'plan-tests-compact', 'strict-clippy-compact', 'formatting-compact'):
    if label not in reports:
        continue
    manifest = reports[label]['source_manifest_sha256']
    current = json.loads((root / f'source-{manifest}.json').read_text())
    reports[label]['source_delta_from_broad_start'] = [name for name in sorted(set(baseline) | set(current)) if baseline.get(name) != current.get(name)]

print(json.dumps(reports, indent=2))
