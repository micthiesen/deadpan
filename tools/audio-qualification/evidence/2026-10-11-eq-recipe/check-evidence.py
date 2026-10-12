from pathlib import Path
import gzip
import hashlib
import json
import math
import re
import sys

root=Path(__file__).parent
def read(name): return json.loads((root/name).read_text())
def log(name): return gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()
for name,record in read('manifest.json').items():
    data=(root/name).read_bytes()
    assert len(data)==record['bytes'] and hashlib.sha256(data).hexdigest()==record['sha256'],name
for name in ['format','eq-tests','final-format','full-tests','clippy','release-build','response','format-check','whitespace']:
    result=read(name+'.json')
    assert result['exit']==0,name
    assert hashlib.sha256(log(name).encode()).hexdigest()==result['log_sha256'],name
tests=log('full-tests')
audio,core=tests.split('Running unittests src/lib.rs (target/debug/deps/deadpan_core-',1)
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',audio)))==523
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',core)))==1051
assert 'test result: FAILED' not in tests
report=json.loads(log('response'))
assert not report['editor_integrated'] and report['taps']==4097
assert report['history_frames']==report['look_ahead_frames']==2048
cases={case['name']:case for case in report['cases']}
def at(name,hz): return next(p['amplitude'] for p in cases[name]['response'] if p['hz']==hz)
assert len(cases)==6
for case in cases.values():
    assert case['maximum_coefficient']<=16
    for point in case['response']:
        assert all(math.isfinite(point[k]) for k in ['hz','real','imaginary','amplitude'])
        assert abs(point['imaginary'])<1e-11
assert all(point['amplitude']==1 for point in cases['flat']['response'])
for hz,db in [(100,6),(1000,-6),(12000,3)]:
    assert abs(at('three_bands',hz)/10**(db/20)-1)<3e-5
assert at('speech_band',100)<1e-4 and at('speech_band',12000)<1e-4
assert abs(at('speech_band',1000)-1)<1e-4
assert abs(at('lowest_high_cut',80)-0.5)<0.001
assert abs(at('highest_low_cut',20000)-0.5)<0.001
assert abs(at('narrow_passband',1040)-1)<0.002
assert at('narrow_passband',920)<1e-4 and at('narrow_passband',1160)<1e-4
if '--source' in sys.argv:
    repo=Path('/Users/michael/Code/deadpan')
    for name,digest in read('identity.json')['source_sha256'].items():
        assert hashlib.sha256((repo/name).read_bytes()).hexdigest()==digest,name
print('PASS: evidence hashes, 1574 tests, six release EQ responses and source identity')
