from pathlib import Path
import gzip
import hashlib
import json
import re
import sys

root = Path(__file__).parent
def read(name): return json.loads((root/name).read_text())
def log(name): return gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()
for name, item in read('manifest.json').items():
    data = (root/name).read_bytes()
    assert len(data) == item['bytes'] and hashlib.sha256(data).hexdigest() == item['sha256'], name
for name in ['dsp-tests','final-audio-tests','final-clippy','final-release-build',
             'final-abi-build','final-abi-sanitized','final-format','final-whitespace',
             'final-measurement-1','final-measurement-2','final-measurement-3']:
    record = read(name+'.json')
    assert record['exit'] == 0, name
    assert hashlib.sha256(log(name).encode()).hexdigest() == record['log_sha256'], name
assert read('clippy.json')['exit'] == 101
assert 'needless_range_loop' in log('clippy')
tests = log('final-audio-tests')
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',tests))) == 540
assert '0 failed' in tests and 'test result: FAILED' not in tests
abi = json.loads(log('final-abi-sanitized'))
assert abi == dict(creation_allocation_failure_sites=15, processed_tiles=100,
                   processing_allocations=0, leaked_allocations=0, guards='passed')
measurements = read('measurements.json')
for run in measurements['runs']:
    actual = json.loads(log(run['name']).splitlines()[0])
    assert actual == run['result']
    assert actual['tile_count'] == 5000 and actual['frames_per_tile'] == 1024
    assert actual['maximum_direct_error'] < 2e-12 and not actual['device_output']
    assert actual['p50_tile_ns'] <= actual['p95_tile_ns'] <= actual['maximum_tile_ns']
assert len(measurements['binary_sha256']) == 64
if '--source' in sys.argv:
    repo = Path('/Users/michael/Code/deadpan')
    for name, digest in measurements['source_sha256'].items():
        assert hashlib.sha256((repo/name).read_bytes()).hexdigest() == digest, name
print('PASS: evidence hashes, 540 tests, sanitizer/allocation witness and three pinned release measurements')
