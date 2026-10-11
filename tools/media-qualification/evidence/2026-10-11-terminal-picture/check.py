from pathlib import Path
import gzip,hashlib,json,re
r=Path(__file__).parent;repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-terminal-picture'
def sha(data):return hashlib.sha256(data).hexdigest()
def read(path):return json.loads(path.read_text())
pins=read(r/'pins.json')
for name,digest in pins['source'].items():
 assert sha((repo/name).read_bytes())==digest,name
 assert sha((r/'source'/name).read_bytes())==digest,name
for name,digest in pins['binary'].items():assert sha((r/'bin'/name).read_bytes())==digest,name
for name in ['clippy-final','format-check','app-build','normal-check','transport-tests-final','presentation-tests-final','app-tests']:
 result=read(r/(name+'.json'));assert result['exit']==0,name
 assert sha((r/(name+'.log')).read_bytes())==result['log_sha256'],name
for name,count in [('transport-tests-final',20),('presentation-tests-final',22)]:
 matches=re.findall(r'test result: ok\. (\d+) passed;', (r/(name+'.log')).read_text());assert sum(map(int,matches))==count,(name,matches)
checks=0;warnings=0
regressions=['terminal-playback','playback-feedback','original-playback','sound-playback','place-slice','trim','gain','original-layout','original-layout-long']
cases=read(r/'cases.json')
for name in regressions+[case[0] for case in cases]:
 report=read(r/name/'report.json')
 assert report['metadata']['binary_sha256']==pins['binary']['deadpan-app'],name
 result=read(r/(name+'.json'))
 assert result['exit']==(1 if name=='gain' else 0),(name,result['exit'])
 for scenario in report['scenarios']:
  for check in scenario['checks']:assert check['passed'],(name,check['name'])
  assert not any(f['severity'] in ['error','failure'] for f in scenario['findings']) or name=='gain',name
  warnings+=sum(f['severity']=='warning' for f in scenario['findings']);checks+=len(scenario['checks'])
fixtures={'1080p':'/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan','4k':'/tmp/deadpan-generated-loops-20261011/4k/two-holds.deadpan','long1080p':'/tmp/deadpan-generated-playback-performance-20261011/1080p-fallback.deadpan','long4k':'/tmp/deadpan-generated-playback-performance-20261011/4k-fallback.deadpan'}
for name,folder in fixtures.items():
 for relative,pin in read(r/(name+'-fixture-pins.json')).items():
  data=(Path(folder)/relative).read_bytes();assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],(name,relative)
pictures=0;late=0
for name,scenario,size in cases:
 data=read(r/name/(scenario+'.json'))
 assert data['ended'] and not data['missing_frames'],name
 assert not any(data['errors'].values()),name
 assert data['audio']['underruns']==0 and data['audio']['faults']==0,name
 if not data['looping']:assert data['last_sample']==data['duration_samples'],name
 for picture in data['pictures']:
  assert 'after_audio_end' in picture
  if picture['after_audio_end']:
   assert not data['looping'] and picture['frame']==data['frames']-1,(name,picture)
   assert picture['heard_sample']==data['duration_samples'],(name,picture)
   late+=1
 pictures+=len(data['pictures'])
# The Gain waveform visibility failure predates this change, at the same
# stopped state and with byte-identical failure details. It is not a pass.
def gain_failure(name):
 report=read(r/name/'report.json')
 scenario=next(s for s in report['scenarios'] if s['name']=='gain')
 assert scenario['steps'][-1]['semantic']['playback'] is None
 return [f for f in scenario['findings'] if f['severity']=='failure']
assert gain_failure('gain')==gain_failure('gain-before')
assert len(gain_failure('gain'))==1
assert gain_failure('gain')[0]['message'].startswith('Gain graph did not fit after bounded measured scrolling:')
assert read(r/'gain-before.json')['exit']==1
assert read(r/'gain-before/report.json')['metadata']['binary_sha256']=='e7ff8d67bfb4a004eb5c00f63e23725886141f5edd94e7e2bc9183d7abe6034f'
# Keep the previous strict during-playback failure intact.
old=repo/'tools/media-qualification/evidence/2026-10-11-generated-slices/play-long/generated-playback.json.gz'
assert json.loads(gzip.decompress(old.read_bytes()))['missing_frames']==[1829]
if (root/'manifest.json').exists():
 for row in read(root/'manifest.json'):
  data=(root/row['path']).read_bytes();assert len(data)==row['bytes'] and sha(data)==row['sha256'],row['path']
  if row['path'].endswith('.gz'):data=gzip.decompress(data)
  assert sha(data)==row['original_sha256'],row['path']
print(json.dumps(dict(sources=len(pins['source']),checks=checks,warnings=warnings,pictures=pictures,after_audio_end=late)))
