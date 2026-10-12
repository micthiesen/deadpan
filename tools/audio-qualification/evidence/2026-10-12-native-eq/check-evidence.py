"""Check native EQ evidence, with optional current-source verification."""
from pathlib import Path
import argparse, gzip, hashlib, json

parser = argparse.ArgumentParser()
parser.add_argument('--source', action='store_true')
args = parser.parse_args()
root = Path(__file__).resolve().parent
repo = root.parents[3]
read = lambda name: json.loads((root/name).read_text())
for name, digest in read('manifest.json').items():
    assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, name
app = gzip.decompress((root/'app-tests.log.gz').read_bytes()).decode()
assert '1120 passed; 0 failed; 9 ignored' in app
for name, count in [('gain-tests', 33), ('eq-tests', 2), ('registry-tests', 11)]:
    log = gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()
    assert f'{count} passed; 0 failed; 0 ignored' in log, name
commands = ['gain-tests','eq-tests','registry-tests','clippy','normal-app','format','release-final','equalizer','gain-final','audio-treatments','equalizer-performance']
for name in commands:
    record = read(name+'.json')
    assert record['exit'] == 0, name
    log = gzip.decompress((root/(name+'.log.gz')).read_bytes())
    assert hashlib.sha256(log).hexdigest() == record['log_sha256'], name
for name, scenario_name, count in [('equalizer','equalizer',81),('gain-final','gain',296),('audio-treatments','audio-treatments',123),('equalizer-performance','equalizer',81)]:
    report = json.loads(gzip.decompress((root/(name+'-report.json.gz')).read_bytes()))
    assert not report['failed'], name
    assert report['metadata']['binary_sha256'] == read('binary-pins.json')['deadpan-app'], name
    found = set()
    for scenario in report['scenarios']:
        assert not scenario['failed'] and all(c['passed'] for c in scenario['checks']), name
        found.add(scenario['name'])
        if scenario['name'] == scenario_name:
            assert len(scenario['checks']) == count, name
        if scenario['name'] == 'kestrel-shortcuts':
            audit = scenario['checks'][0]['actual']
            assert audit['routing_cases'] == 21884016 and audit['reserved_bindings'] == 62
            assert not audit['conflicts'] and audit['live_source_sha256'] is None
    assert found == {scenario_name, 'kestrel-shortcuts'}, name
if args.source:
    for path, digest in read('source-pins.json')['files'].items():
        assert hashlib.sha256((repo/path).read_bytes()).hexdigest() == digest, path
print('Native EQ evidence verified' + (' against current source' if args.source else ''))
