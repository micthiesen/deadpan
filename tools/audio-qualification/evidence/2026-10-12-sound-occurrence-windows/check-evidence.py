import argparse, gzip, hashlib, json, re
from pathlib import Path
p = argparse.ArgumentParser()
p.add_argument('--source', action='store_true')
args = p.parse_args()
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
for name in ['windows', 'pcm-bounded', 'fmt-check', 'plan-audio-playback', 'clippy', 'release-windows', 'release-pcm', 'release']:
    assert commands[name]['exit'] == 0, name
assert commands['pcm-windows']['exit'] != 0 and 'Err` value: Range' in logs['pcm-windows']
assert '4 passed; 0 failed' in logs['release-windows']
assert '1 passed; 0 failed' in logs['release-pcm']
results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', logs['plan-audio-playback'])
assert results and not any(int(row[1]) or int(row[2]) for row in results)
assert read('test-counts.json') == dict(passed=sum(int(row[0]) for row in results), failed=0, ignored=0, result_groups=len(results))
if args.source:
    for name, digest in read('identity.json')['sources'].items():
        assert sha(repo/name) == digest, name
print('Occurrence window evidence verified' + (' against current source' if args.source else ''))
