from pathlib import Path
import gzip,hashlib,json,re
root=Path(__file__).resolve().parent
repo=root.parents[3]
def read(name):
 p=root/name
 return json.loads(gzip.decompress(p.read_bytes()) if p.suffix=='.gz' else p.read_text())
def log(name):return gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()
def totals(name):
 rows=[tuple(map(int,r)) for r in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log(name))]
 assert rows and all(f==i==0 for _,f,i in rows),name
 return sum(p for p,_,_ in rows)
for name,meta in read('manifest.json').items():
 data=(root/name).read_bytes();assert len(data)==meta['bytes'] and hashlib.sha256(data).hexdigest()==meta['sha256'],name
for name,digest in read('source-pins.json')['files'].items():assert hashlib.sha256((repo/name).read_bytes()).hexdigest()==digest,name
for path in root.glob('*.log.gz'):
 receipt=read(path.name.removesuffix('.log.gz')+'.json')
 assert hashlib.sha256(gzip.decompress(path.read_bytes())).hexdigest()==receipt['log_sha256'],path
for name in ['check','focused-delivery-fixed','suites','doctor','format','format-final','clippy-final','release-build','replays','format-check']:
 assert read(name+'.json')['exit']==0,name
for name in ['focused','focused-fixed','focused-clock-fixed','clippy','control']:
 assert read(name+'.json')['exit']!=0,name
assert totals('suites')==92
assert totals('doctor')==3
assert 'closing_the_store_revokes_a_full_prepared_fifo_before_more_delivery ... ok' in log('suites')
assert 'the future Preserve must finish while the heard clock is still at its initial boundary' in log('control')
assert 'test result: FAILED. 0 passed; 1 failed; 0 ignored' in log('control')
assert read('control-source.json')['queue_sha256']!=read('source-pins.json')['files']['crates/deadpan-playback/src/preparation/queue.rs']
for name in ['suites','doctor','control']:
 pins=read(name+'-binaries.json');assert pins and all(len(v)==64 for v in pins.values())
for name in ['real-once','real-loop']:
 meta=read(name+'.json');report=read(name+'-report.json')
 assert meta['exit']==0 and report['covered_requested_interval'] and report['failure'] is None,name
 for suffix,key in [('-report.json','stdout_sha256'),('.stderr.gz','stderr_sha256')]:
  data=(root/(name+suffix)).read_bytes()
  if suffix.endswith('.gz'):data=gzip.decompress(data)
  assert hashlib.sha256(data).hexdigest()==meta[key],name
 audio=report['audio']
 assert audio['underruns_starved']==audio['device_faults']==0,name
 assert audio['queue_depths']['playback_prefill_frames']['current']==8192
 frames=audio['queue_depths']['playback_prepared_frames']
 assert frames['current']==0 and frames['high']<=1433600
 assert any(x['queued_frames']>8*48000 and x['heard_sample'] is not None and x['heard_sample']<240*1001*48000//30000 for x in audio['preparation_lead']),name
 assert read(name+'-host.json')['perf_sha256']==read('release-binaries.json')['target/release/examples/perf']
assert read('real-loop-report.json')['audio']['queue_depths']['playback_prepared_frames']['high']==1433600
replays=read('replay-summary.json')
assert len(replays)==3
for entry in replays:
 report=read('replay-'+entry['scenario']+'.json.gz')
 assert not report['failed'] and not entry['failed']
 assert report['metadata']['binary_sha256']==entry['binary_sha256']
 checks=[c for s in report['scenarios'] for c in s['checks']]
 assert len(checks)==entry['checks'] and all(c['passed'] for c in checks)
 for image in entry['selected_images']:assert (root/image['file']).is_file()
# Export must include the additional queue gauge, not just a painted label.
assert 'playback_prepared_frames' in json.dumps(read('exported-diagnostics.json'))
for name in ['docs/PLAYBACK.md','docs/AUDIO_STAGE_PREPARATION.md','docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md']:
 assert 'audio-ahead-2026-10-11.md' in (repo/name).read_text(),name
print(json.dumps(dict(passed=True,playback=88,diagnostics=4,doctor=3,negative_control='expected failure',real_device_runs=2,replay_checks=sum(e['checks'] for e in replays))))
