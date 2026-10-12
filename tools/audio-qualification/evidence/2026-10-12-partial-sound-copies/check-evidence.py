import argparse, gzip, hashlib, json, re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--source', action='store_true')
args = parser.parse_args()
here = Path(__file__).resolve().parent
repo = here.parents[3]

def read(name):
    return json.loads((here/name).read_text())

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

manifest = read('manifest.json')
assert set(manifest) == {path.name for path in here.iterdir() if path.is_file() and path.name != 'manifest.json'}
for name, digest in manifest.items():
    assert sha(here/name) == digest, name
commands = {record['name']: record for record in read('commands.json')}
logs = {}
for name, record in commands.items():
    raw = gzip.decompress((here/(name+'.log.gz')).read_bytes())
    assert hashlib.sha256(raw).hexdigest() == record['log_sha256'], name
    logs[name] = raw.decode()
for name in ['all-audio-core-store', 'history-grid-tests', 'history-boundary-oracles', 'shared-half-sample', 'context-alias', 'core-sound-clock-integration', 'partial-store-fixed', 'fmt-check', 'clippy', 'release-partial', 'release-store', 'release', 'release-export']:
    assert commands[name]['exit'] == 0, name
assert commands['partial-pcm']['exit'] != 0
assert commands['wrapper-active-before']['exit'] != 0
assert commands['sound-clocks-current']['exit'] != 0
assert commands['nested-partial']['exit'] != 0 and 'Range endpoints require' in logs['nested-partial']
assert commands['clippy-before']['exit'] != 0 and 'unused import' in logs['clippy-before']
assert '7 passed; 0 failed' in logs['release-partial']
assert '1 passed; 0 failed' in logs['release-store']
assert '1 passed; 0 failed' in logs['release-export']
results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', logs['all-audio-core-store'])
assert results and not any(int(row[1]) or int(row[2]) for row in results)
assert read('test-counts.json') == dict(passed=sum(int(row[0]) for row in results), failed=0, ignored=0, result_groups=len(results))
report = read('export-results.json')['report']
assert report['passed'] and report['summary']['nonzero_offsets'] == 0
assert report['summary']['pictures_checked'] == 7
assert report['summary']['blocks_compared'] == 19
assert report['summary']['signal_windows'] > 0
assert read('export-artifact.json')['sha256'] == sha(here/'partial-sounds.mp4')
if args.source:
    for name, digest in read('identity.json')['sources'].items():
        assert sha(repo/name) == digest, name
print('Partial sound copy evidence verified' + (' against current source' if args.source else ''))
