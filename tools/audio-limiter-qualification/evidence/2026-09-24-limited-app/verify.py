"""Verify retained application evidence without opening a project or device."""
import gzip
import hashlib
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parent
manifest = json.loads((root / 'manifest.json').read_text())
actual = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()}
assert actual == set(manifest) | {'manifest.json'}
for relative, record in manifest.items():
    path = root / relative
    assert path.resolve().is_relative_to(root) and not path.is_symlink()
    data = path.read_bytes()
    assert len(data) == record['bytes']
    assert hashlib.sha256(data).hexdigest() == record['sha256']

gate = root / 'gate/final-3'
report = json.loads((gate / 'report.json').read_text())
assert len(report['commands']) == 5
assert all(c['exit_code'] == 0 for c in report['commands'])
log = gzip.decompress((gate / '2.log.gz').read_bytes()).decode()
rows = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
counts = dict(zip(('passed', 'failed', 'ignored'), [sum(int(r[i]) for r in rows) for i in range(3)]))
assert counts == report['tests'] == {'passed': 1265, 'failed': 0, 'ignored': 0}
before = json.loads((gate / 'source-before.json').read_text())
after = json.loads((gate / 'source-after.json').read_text())
assert before.keys() == after.keys() and len(before) == report['source_count'] == 400
changed = [p for p in before if before[p] != after[p]]
assert changed == ['crates/deadpan-playback/README.md']
old = (gate / 'readme-before.txt').read_bytes()
new = (gate / 'readme-after.txt').read_bytes()
assert old.replace(b'at most8192frames', b'at most 8192 frames') == new
assert hashlib.sha256(old).hexdigest() == before[changed[0]]
assert hashlib.sha256(new).hexdigest() == after[changed[0]]

build = json.loads((root / 'native/build.json').read_text())
assert build['exit_code'] == 0 and build['source_unchanged']
assert build['binary_sha256'] == build['bundle_sha256']
environment = json.loads((root / 'native/environment.json').read_text())
assert 'release: 1.97.1' in environment['rustc_version']
project_before = json.loads((root / 'native/project-before.json').read_text())
project_after = json.loads((root / 'native/project-after.json').read_text())
assert project_before == project_after
print(json.dumps({'manifest_files': len(manifest), 'tests': counts, 'readme_only_change': True, 'project_dump_unchanged': True}))
