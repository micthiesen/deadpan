from pathlib import Path
import gzip,hashlib,json,re,sys
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-slices'
def sha(data):return hashlib.sha256(data).hexdigest()
manifest=json.loads((root/'manifest.json').read_text());raw={}
assert len({row['path'] for row in manifest})==len(manifest)
for row in manifest:
 data=(root/row['path']).read_bytes()
 assert len(data)==row['bytes'] and sha(data)==row['sha256'],row['path']
 plain=gzip.decompress(data) if row['path'].endswith('.gz') else data
 assert sha(plain)==row['original_sha256'],row['path']
 raw[row['path'].removesuffix('.gz')]=plain
def read(name):return json.loads(raw[name])
identity=read('identity.json');pins=read('playback-pins.json');compare=read('compare-pins.json')
for label,source in [('source',pins['source']),('compare-source',compare['source'])]:
 for name,expected in source.items():
  assert sha(raw[label+'/'+name])==expected,name
  if '--current' in sys.argv:assert sha((repo/name).read_bytes())==expected,name
for name,expected in pins['binary'].items():assert sha(Path(name).read_bytes())==expected,name
assert sha(Path(compare['binary']).read_bytes())==compare['binary_sha256']
for group in ['libraries','upstream_sources','test_binaries']:
 for name,expected in identity[group].items():assert sha(Path(name).read_bytes())==expected,name
for name,path in identity['fixtures'].items():
 for relative,pin in read(name+'-fixture-pins.json').items():
  data=(Path(path)/relative).read_bytes()
  assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],relative
for name in ['native-tests-fixed','compare-build-fixed','native-full','worker-tests','fixture-tests','slices-4k-test','clippy','format-check','sanitizers']:
 value=read(name+'.json');assert value['exit']==0,name
 assert sha(raw[name+'.log'])==value['log_sha256'],name
for name,count in [('worker-tests',45),('fixture-tests',7),('slices-4k-test',1)]:assert f'{count} passed; 0 failed'.encode() in raw[name+'.log'],name
assert sum(map(int,re.findall(rb'test result: ok\. (\d+) passed',raw['native-full.log'])))==183
san=read('sanitizers/report.json');assert san['exit_code']==0 and san['failure'] is None
assert sha(raw['sanitizers/test.log'])==san['test_log_sha256']
for name,expected in san['test_binary_sha256'].items():assert sha(Path(name).read_bytes())==expected,name
assert sha(Path(san['runtime']).read_bytes())==san['runtime_sha256']
micro_pictures=0
for size in ['1080p','4k']:
 for i in range(2):
  name=f'compare-{size}-{i}'
  result=read(name+'.json');assert result['exit']==0
  assert sha(raw[name+'.log'])==result['log_sha256']
  v=read(name+'-report.json');assert len(v['cases'])==18
  assert sha(Path(v['source']).read_bytes())==v['sha256']
  assert len(v['reference_pictures'])==v['pictures']
  for c in v['cases']:
   assert not c['frame_threading'] and c['slice_threading']==(c['threads']>1)
   assert len(c['sequential_ms'])==v['pictures'] and len(c['seeks'])==50
   micro_pictures+=v['pictures']+50
names=[f'play-1080p-{i}' for i in range(5)]+['play-4k','loop-1080p','loop-4k','play-long']
pictures=0
for name in names:
 result=read(name+'.json');assert result['exit']==(1 if name=='play-long' else 0),name
 assert sha(raw[name+'.log'])==result['log_sha256'],name
 scenario='generated-loop' if name.startswith('loop-') else 'generated-playback'
 v=read(name+'/'+scenario+'.json');report=read(name+'/report.json')
 assert v['ended'] and v['missing_frames']==([1829] if name=='play-long' else []),name
 assert v['audio']['faults']==v['audio']['underruns']==0,name
 assert v['audio']['silent_padding_frames']==(0 if v['looping'] else 1344 if name=='play-long' else 1408),name
 assert v['native_picture_counts']['skipped']==v['native_picture_counts']['reduced']==0,name
 assert all(error is None for error in v['errors'].values()),name
 assert report['metadata']['binary_sha256']==next(h for p,h in pins['binary'].items() if p.endswith('/deadpan-app'))
 size='long' if name=='play-long' else ('4k' if name.endswith('4k') else '1080p')
 assert report['metadata']['fixture_sha256']==read(size+'-fixture-pins.json')['project.sqlite']['sha256']
 for s in report['scenarios']:
  failed=[c['name'] for c in s['checks'] if not c['passed']]
  expected=['Every Original and Generated frame reaches Metal during playback'] if name=='play-long' and s['name']=='generated-playback' else []
  assert failed==expected,(name,failed)
  assert len([f for f in s['findings'] if f['severity']=='failure'])==len(expected),name
 if v['looping']:
  start,end=v['window_frames']
  assert v['expected_laps']==30 and v['pictures'][-1]['heard_lap']==30
  assert not v['lost_device_lap'] and v['stop_preserved_picture']
  assert {(p['lap'],p['frame']) for p in v['pictures']}=={(lap,frame) for lap in range(30) for frame in range(start,end)}|{(30,start)}
 else:assert {p['frame'] for p in v['pictures']}==set(range(v['frames']))-({1829} if name=='play-long' else set())
 assert read('measurements.json')['playback'][name]['timing_pairs']==len(v['pictures'])
 pictures+=len(v['pictures'])
terminal=read('terminal.json')
assert terminal['remaining_samples']==236
assert any(e['stage']=='picture_superseded' for e in terminal['events'])
assert 'transport: None' in terminal['final_picture']['ticket']
assert 'ProjectFrame(1829)' in terminal['final_picture']['location']
links=0
for name in ['docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/PLAYBACK.md','docs/PERFORMANCE.md','native/deadpan-source/README.md','docs/qualification/generated-slices-2026-10-11.md']:
 page=repo/name
 for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',page.read_text()):
  if ':' in target or target.startswith('#'):continue
  path=target.split('#',1)[0].strip('<>')
  if path:assert (page.parent/path).exists(),(name,target)
  links+=1
print(json.dumps(dict(files=len(manifest),sources=len(pins['source']),micro_picture_comparisons=micro_pictures,playback_pictures=pictures,links=links)))
