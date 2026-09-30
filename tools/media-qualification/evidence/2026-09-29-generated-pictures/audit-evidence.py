"""Verify the retained evidence without extracting generated media."""
import gzip
import hashlib
import json
from pathlib import Path
import tarfile

root = Path(__file__).resolve().parent
manifest = json.loads((root / 'manifest.json').read_text())
actual = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file() and p.name != 'manifest.json'}
assert actual == set(manifest), 'file inventory changed'
for name, expected in manifest.items():
    wire = (root / name).read_bytes()
    assert len(wire) == expected['bytes'], name
    assert hashlib.sha256(wire).hexdigest() == expected['sha256'], name

commands = json.loads((root / 'verification.json').read_text())
for command in commands:
    name = command['name']
    journal = json.loads((root / 'commands' / (name + '.json')).read_text())
    assert journal['exit_code'] == command['exit_code'], name
    key = journal['source_manifest_sha256']
    wire = gzip.decompress((root / 'sources' / ('source-' + key + '.json.gz')).read_bytes())
    assert hashlib.sha256(wire).hexdigest() == key, name

summaries = json.loads((root / 'summary.json').read_text())
checks = 0
for name, summary in summaries.items():
    report = json.loads(gzip.decompress((root / 'reports' / (name + '.json.gz')).read_bytes()))
    assert report['failed'] == summary['failed'], name
    assert len(report['scenarios']) == len(summary['scenarios']), name
    for scenario, retained in zip(report['scenarios'], summary['scenarios'], strict=True):
        assert scenario['name'] == retained['name'], name
        assert len(scenario['checks']) == retained['checks'], name
        assert [c for c in scenario['checks'] if not c['passed']] == retained['failed_checks'], name
        checks += len(scenario['checks'])

objects = json.loads((root / 'generated-objects.json').read_text())
with tarfile.open(root / 'generated-objects.tar.gz', 'r:gz') as archive:
    assert {m.name for m in archive.getmembers()} == set(objects)
    for member in archive.getmembers():
        assert member.isfile()
        wire = archive.extractfile(member).read()
        assert len(wire) == objects[member.name]['bytes']
        assert hashlib.sha256(wire).hexdigest() == objects[member.name]['sha256']
print(json.dumps({'files':len(manifest), 'commands':len(commands), 'reports':len(summaries),
                  'recorded_checks_including_prior_runs':checks, 'generated_objects':len(objects)}))
