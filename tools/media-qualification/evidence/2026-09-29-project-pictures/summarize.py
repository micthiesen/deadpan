import hashlib
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parent
results = {}
for name in ('workspace', 'picture-corrected', 'picture-final', 'feature-tests'):
    command = json.loads((root / (name + '.json')).read_text())
    log = (root / (name + '.log')).read_text()
    rows = [tuple(map(int, row)) for row in re.findall(
        r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out;', log)]
    results[name] = {'exit_code': command['exit_code'], 'seconds': command['seconds'],
                     'target_results': len(rows),
                     **{key: sum(row[index] for row in rows)
                        for index, key in enumerate(('passed', 'failed', 'ignored', 'filtered'))},
                     'source_manifest_sha256': command['source_manifest_sha256']}
assert results['workspace']['failed'] == 2 and results['workspace']['exit_code'] == 101
assert results['picture-corrected']['failed'] == 1 and results['picture-corrected']['exit_code'] == 101
assert results['picture-final']['failed'] == 0 and results['picture-final']['passed'] == 8
assert results['feature-tests']['failed'] == 0 and results['feature-tests']['exit_code'] == 0
before = json.loads((root / ('source-' + results['workspace']['source_manifest_sha256'] + '.json')).read_text())
after = json.loads((root / ('source-' + results['picture-final']['source_manifest_sha256'] + '.json')).read_text())
changed = [name for name in sorted(set(before) | set(after)) if before.get(name) != after.get(name)]
assert changed == ['crates/deadpan-cli/src/picture/tests.rs'], changed
for name, expected in after.items():
    path = Path('/Users/michael/Code/deadpan') / name
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, name
report = {'tests': results, 'source_changes_after_full_gate': changed,
          'current_sources_match_corrected_checks': True,
          'continuation': 'Full failed run retained; only the corrected picture module was rerun; unrelated passing suites and doctests retained.'}
(root / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
