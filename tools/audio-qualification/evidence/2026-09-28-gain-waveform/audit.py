"""Audit the retained waveform evidence independently of the writer."""
import gzip
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
manifest = json.loads((root / 'manifest.json').read_text())
actual = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file() and p.name != 'manifest.json'}
assert actual == set(manifest), (actual - set(manifest), set(manifest) - actual)
for name, expected in manifest.items():
    data = (root / name).read_bytes()
    assert len(data) == expected['bytes'], name
    assert hashlib.sha256(data).hexdigest() == expected['sha256'], name
summary = json.loads((root / 'summary.json').read_text())
for row in summary['commands']:
    retained = json.loads((root / 'commands' / (row['name'] + '.json')).read_text())
    assert {k: v for k, v in row.items() if k != 'name'} == retained
    assert 'exit_code' in retained and retained['seconds'] >= 0
    gzip.decompress((root / 'commands' / (row['name'] + '.log.gz')).read_bytes())
    source = gzip.decompress((root / 'sources' / (row['source_manifest_sha256'] + '.json.gz')).read_bytes())
    assert hashlib.sha256(source).hexdigest() == row['source_manifest_sha256']
for row in summary['replays']:
    report = json.loads(gzip.decompress((root / 'replays' / (row['name'] + '.json.gz')).read_bytes()))
    assert report['failed'] == row['failed']
    assert len(report['scenarios']) == len(row['scenarios'])
    for actual, expected in zip(report['scenarios'], row['scenarios']):
        assert actual['name'] == expected['name']
        assert len(actual['checks']) == expected['checks']
        assert [c for c in actual['checks'] if not c['passed']] == expected['failed_checks']
for row in summary['images']:
    assert (root / 'images' / row['name']).is_file()
print(json.dumps({'files': len(manifest), 'bytes': sum(row['bytes'] for row in manifest.values()),
    'commands': len(summary['commands']), 'failed_commands': sum(r['exit_code'] != 0 for r in summary['commands']),
    'replays': len(summary['replays']), 'images': len(summary['images'])}))
