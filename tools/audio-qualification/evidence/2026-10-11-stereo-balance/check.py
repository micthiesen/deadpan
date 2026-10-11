from pathlib import Path
import gzip,hashlib,json,re
root=Path(__file__).resolve().parent
repo=root.parents[3]
read=lambda name:json.loads((root/name).read_text())
manifest=read('manifest.json')
for path,meta in manifest.items():
 data=(root/path).read_bytes()
 assert len(data)==meta['bytes'] and hashlib.sha256(data).hexdigest()==meta['sha256'],path
pins=read('source-pins.json')
for path,digest in pins['files'].items():
 assert hashlib.sha256((repo/path).read_bytes()).hexdigest()==digest,path
for name in ['core-final','takes-bound','clippy','normal-app-check','release-build','export','format-check','audio-treatments','gain']:
 record=read(name+'.json');assert record['exit']==0,(name,record)
 log=gzip.decompress((root/(name+'.log.gz')).read_bytes())
 assert hashlib.sha256(log).hexdigest()==record['log_sha256'],name
checks=0
for name,expected in [('audio-treatments',123),('gain',296)]:
 report=json.loads(gzip.decompress((root/(name+'-report.json.gz')).read_bytes()))
 assert not report['failed']
 assert report['metadata']['binary_sha256']==read('binary-pins.json')['deadpan-app']
 for scenario in report['scenarios']:
  assert not scenario['failed'] and all(c['passed'] for c in scenario['checks'])
  if scenario['name']==name:assert len(scenario['checks'])==expected
  if scenario['name']=='kestrel-shortcuts':
   audit=scenario['checks'][0]['actual']
   assert audit['routing_cases']==21884016 and audit['reserved_bindings']==62 and not audit['conflicts']
   assert audit['live_source_sha256'] is None
 checks+=expected
export=read('export-report.json')['report']
assert export['passed'] and export['summary']['pictures_checked']==54
assert export['summary']['audio_windows_checked']==2
assert all(w['offset_status']=='verified_zero' and w['measured_offset_samples']==0 and not w['flags'] for w in export['audio'])
channels=read('encoded-channels.json')
assert channels['passed'] and channels['no_alignment_applied'] and channels['decoded_frames']==86486
assert channels['windows'][0]['peaks'][1]==0
assert abs(channels['windows'][1]['right_over_left']-2)<0.02
assert hashlib.sha256((root/'stereo-balance.mp4').read_bytes()).hexdigest()==channels['movie_sha256']
assert hashlib.sha256(gzip.decompress((root/'decoded-aac.f32le.gz').read_bytes())).hexdigest()==channels['pcm_sha256']
app=gzip.decompress((root/'app-final.log.gz').read_bytes()).decode()
assert '1116 passed; 1 failed; 9 ignored' in app
assert 'named_takes_restore_as_one_edit_and_metadata_preserves_head_and_redo' in app
assert '3 passed; 0 failed' in gzip.decompress((root/'takes-bound.log.gz').read_bytes()).decode()
assert read('audio-test-summary.json')['passed']==481
for path in ['docs/AUDIO_GAIN.md','docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/KEYBINDING_COMPATIBILITY.md']:
 assert 'stereo-balance-2026-10-11.md' in (repo/path).read_text(),path
print(json.dumps({'passed':True,'source_files':len(pins['files']),'replay_checks':checks,'pictures':54,'audio_windows':2,'manifest_files':len(manifest)}))
