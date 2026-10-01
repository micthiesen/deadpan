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
    if 'command' not in report or not path.with_suffix('.log').exists():
        continue
    log = path.with_suffix('.log').read_text()
    report['tests'] = counts(log)
    report['unit_and_integration'] = counts(log.split('Doc-tests ')[0])
    report['doc_tests'] = counts(log[log.index('Doc-tests '):]) if 'Doc-tests ' in log else {'targets': 0}
    reports[path.stem] = report
broad = (root / 'affected-tests-corrected.log').read_text() if (root / 'affected-tests-corrected.log').exists() else ''
marker = 'Running unittests src/lib.rs (target/debug/deps/deadpan_core-'
if marker in broad:
    assert broad.count(marker) == 1
    reports['verified_audio_cli'] = counts(broad.split(marker)[0])
if reports.get('remaining-tests', {}).get('exit_code') == 0:
    reports['distinct_affected_tests'] = {
        key: reports['verified_audio_cli'][key] + reports['remaining-tests']['unit_and_integration'][key]
        for key in ('targets', 'passed', 'failed', 'ignored')
    }
print(json.dumps(reports, indent=2))
