from pathlib import Path
import gzip,json,math,re
r=Path(__file__).parent
def dist(values):
 values=sorted(values)
 return dict(count=len(values),p95=values[math.ceil(len(values)*.95)-1],max=values[-1]) if values else None
micro={}
for p in sorted(r.glob('compare-*-report.json')):
 v=json.loads(p.read_text())
 micro[p.stem]={str(n):dict(sequential_ms=dist([t for c in v['cases'] if c['threads']==n for t in c['sequential_ms'][1:]]),seek_ms=dist([s['ms'] for c in v['cases'] if c['threads']==n for s in c['seeks']])) for n in [1,2,4,8,12,16]}
playback={}
names=[f'play-1080p-{i}' for i in range(5)]+['play-4k','loop-1080p','loop-4k','play-long']
inputs=[(name,r/name,False) for name in names]
inputs.append(('prior-drop',Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-11-generated-loops/control'),True))
for name,folder,compressed in inputs:
 def read(filename):
  p=folder/(filename+('.gz' if compressed else ''))
  return json.loads(gzip.decompress(p.read_bytes()) if compressed else p.read_bytes())
 report=read('report.json');value=read('generated-loop.json' if name.startswith('loop-') else 'generated-playback.json')
 scenario=next(s for s in report['scenarios'] if s['name'] in ['generated-playback','generated-loop'])
 events={}
 for step in scenario['steps']:
  for e in step['semantic'].get('stages',[]):
   if e['ticket'] is not None:events.setdefault(e['ticket'],{})[e['stage']]=e['request_elapsed_ms']
 rows=[]
 for p in value['pictures']:
  e=events[p['ticket']]
  rows.append(dict(p,worker_ms=e['picture_worker_finished']-e['picture_worker_started'],composition_ms=e['picture_composed']))
 generated=[p for p in rows if p['generated']]
 first=generated[0]['frame']
 log=(r/(name+'.log')).read_text() if not compressed else ''
 footprint=re.search(r'(\d+)\s+peak memory footprint',log)
 playback[name]=dict(ended=value['ended'],missing=value['missing_frames'],pictures=len(rows),audio=value['audio'],native=value['native_picture_counts'],
  generated_worker_ms=dist([p['worker_ms'] for p in generated]),composition_ms=dist([p['composition_ms'] for p in rows]),
  first_generated=[p for p in rows if p.get('lap',0)==0 and first<=p['frame']<first+6],
  clock_lag=dist([p['sampled_clock_lag_frames'] for p in rows if p['sampled_clock_lag_frames'] is not None]),
  peak_bytes=int(footprint[1]) if footprint else None,elapsed_ms=value['elapsed_ms'],
  timing_pairs=len(rows),errors=value['errors'])
out=dict(micro=micro,playback=playback)
(r/'measurements.json').write_text(json.dumps(out,indent=2)+'\n')
for name,v in playback.items():print(name,json.dumps({k:v[k] for k in ['missing','pictures','generated_worker_ms','composition_ms','peak_bytes']}))
