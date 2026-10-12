from pathlib import Path
import gzip
import hashlib
import json
import re

root=Path(__file__).parent
def read(name): return json.loads((root/name).read_text())
def log(name): return gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()
manifest=read('manifest.json')
for name, record in manifest.items():
    data=(root/name).read_bytes()
    assert len(data)==record['bytes'] and hashlib.sha256(data).hexdigest()==record['sha256'], name
for name in ['budget-suites','storage-tests','diagnostics-bin','transport-bin','budget-clippy','budget-release','replays','visual-replays']:
    record=read(name+'.json')
    assert record['exit']==0, name
    assert hashlib.sha256(log(name).encode()).hexdigest()==record['log_sha256'], name
suite=log('budget-suites')
audio,playback=suite.split('Running unittests src/lib.rs (target/debug/deps/deadpan_playback',1)
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',audio)))==516
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',playback)))==88
assert len(re.findall(r'test (?:persistent_pcm::tests::|stages::stage_cache::tests::)\S+ \.\.\. ok',audio))==13
for name, count in [('storage-tests',3),('diagnostics-bin',1),('transport-bin',24)]:
    assert f'{count} passed' in log(name), name
for group in ['device-final-runs.json','device-final-long-runs.json']:
    runs=read(group)
    assert all(record['exit']==0 for record in runs['runs'])
for prefix in ['final-', 'final-long-']:
    reports=[read('device-'+prefix+name+'.json') for name in ['cold','restart','disabled']]
    for report in reports:
        assert report['failure'] is None and report['covered_requested_interval']
        assert report['audio']['underruns_starved']==0 and report['audio']['device_faults']==0
    cold,warm,off=[report['audio']['saved_stretch'] for report in reports]
    assert cold['event']=='saved' and cold['regenerated_from']=='not saved yet'
    assert warm['event']=='verified and restored' and warm['key']==cold['key']
    assert warm['key'].startswith('blake3:') and off is None
    if prefix=='final-':
        assert all(report['pictures']['presented']==240 and report['pictures']['trailing_missed']==0 for report in reports)
for name in ['device-long-cold.json','device-long-restart.json']:
    assert read(name)['failure']=='frozen projection work exhausted'
for name in ['original-playback','sound-playback','diagnostics','storage','visual-diagnostics','visual-storage']:
    report=json.loads(gzip.decompress((root/('replay-'+name+'.json.gz')).read_bytes()))
    assert not report['failed']
    assert all(check['passed'] for scenario in report['scenarios'] for check in scenario['checks'])
print('PASS: checksums, final test counts, cached/restarted/disabled playback, retained failures and UI replays')
