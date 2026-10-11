from pathlib import Path
import gzip
import hashlib
import json
import re

root=Path(__file__).resolve().parent
repo=root.parents[3]

def read(name):
 p=root/name
 return json.loads(gzip.decompress(p.read_bytes()) if p.suffix=='.gz' else p.read_text())

def log(name):
 return gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()

def totals(name):
 rows=[tuple(map(int,row)) for row in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log(name))]
 assert rows and all(failed==ignored==0 for _,failed,ignored in rows),name
 return sum(passed for passed,_,_ in rows)

for name,meta in read('manifest.json').items():
 data=(root/name).read_bytes()
 assert len(data)==meta['bytes'] and hashlib.sha256(data).hexdigest()==meta['sha256'],name
for name,digest in read('source-pins.json')['files'].items():
 assert hashlib.sha256((repo/name).read_bytes()).hexdigest()==digest,name
for path in root.glob('*.log.gz'):
 receipt=read(path.name.removesuffix('.log.gz')+'.json')
 assert hashlib.sha256(gzip.decompress(path.read_bytes())).hexdigest()==receipt['log_sha256'],path
for name in ['app-check','warm-tests','real-pcm-final','playback-suite','warming-final',
             'format','format-final','clippy','format-check-final','transport-final',
             'clippy-final','native-replay-final','regression-replays-final']:
 assert read(name+'.json')['exit']==0,name
for name in ['engine-check','native-check','warm-tests-expanded','native-replay','native-replay-fixed','regression-replays']:
 assert read(name+'.json')['exit']!=0,name
assert 'warming did not finish' in log('warm-tests-expanded')
assert totals('playback-suite')==80
assert totals('warming-final')==10
assert totals('transport-final')==21
assert 'idle_horizon_prepares_a_future_preserve_and_seek_reuses_its_canonical_pcm ... ok' in log('warming-final')
assert 'stop_cancels_a_promoted_warm_read_before_output_activation ... ok' in log('warming-final')
assert 'waveform_request_preempts_idle_audio_without_opening_output ... ok' in log('warming-final')
for name in ['playback-suite','warming-final','transport-final']:
 pins=read(name+'-binaries.json')
 assert pins and all(len(v)==64 for v in pins.values()),name
summary=read('replay-summary.json')
assert len(summary)==5
for entry in summary:
 report=read(entry['run']+'-'+entry['scenario']+'.json.gz')
 assert report['metadata']['binary_sha256']==entry['binary_sha256']
 checks=[check for s in report['scenarios'] for check in s['checks']]
 assert len(checks)==entry['checks']
 if entry['run'] in ['native-replay','native-replay-fixed']:
  assert report['failed'] and entry['failed']
  messages=[f['message'] for s in report['scenarios'] for f in s['findings'] if f['severity']=='failure']
  assert any(('parent traversal' if entry['run']=='native-replay' else 'did not become fully visible') in m for m in messages)
 else:
  assert not report['failed'] and not entry['failed']
  assert all(check['passed'] for check in checks)
  for scenario in report['scenarios']:
   assert not scenario['failed']
   if scenario['name']!='kestrel-shortcuts':assert scenario['skipped'], 'retain unmeasured scope'
 for image in entry['selected_images']:assert (root/image['file']).is_file()
final=next(e for e in summary if e['run']=='native-replay-final')
assert final['checks']==26
for name in ['docs/PLAYBACK.md','docs/AUDIO_STAGE_PREPARATION.md','docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md']:
 assert 'audio-warming-2026-10-11.md' in (repo/name).read_text(),name
print(json.dumps({'passed':True,'playback_suite':80,'final_warming_tests':10,'transport_tests':21,
                  'idle_replay_checks':26,'control_replay_checks':sum(e['checks'] for e in summary if e['run']=='regression-replays-final')}))
