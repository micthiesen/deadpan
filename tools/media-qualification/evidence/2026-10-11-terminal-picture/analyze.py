from pathlib import Path
import json,math,re
r=Path(__file__).parent
def dist(values):
 values=sorted(values)
 return dict(count=len(values),p95=values[math.ceil(len(values)*.95)-1],max=values[-1]) if values else None
measurements={}
for name,scenario,_ in json.loads((r/'cases.json').read_text()):
 value=json.loads((r/name/(scenario+'.json')).read_text())
 report=json.loads((r/name/'report.json').read_text())
 replay=next(s for s in report['scenarios'] if s['name']==scenario)
 events={}
 for step in replay['steps']:
  for e in step['semantic'].get('stages',[]):
   if e['ticket'] is not None:events.setdefault(e['ticket'],{})[e['stage']]=e['request_elapsed_ms']
 rows=[]
 for p in value['pictures']:
  e=events[p['ticket']]
  rows.append(dict(p,worker_ms=e['picture_worker_finished']-e['picture_worker_started'],composition_ms=e['picture_composed']))
 late=[p for p in rows if p['after_audio_end']]
 peak=re.search(r'(\d+)\s+peak memory footprint',(r/(name+'.log')).read_text())
 measurements[name]=dict(ended=value['ended'],missing=value['missing_frames'],pictures=len(rows),after_audio_end=late,
  before_end_update=len(rows)-len(late),audio=value['audio'],native=value['native_picture_counts'],last_sample=value['last_sample'],duration_samples=value['duration_samples'],
  composition_ms=dist([p['composition_ms'] for p in rows]),clock_lag=dist([p['sampled_clock_lag_frames'] for p in rows if p['sampled_clock_lag_frames'] is not None]),
  peak_bytes=int(peak[1]) if peak else None,errors=value['errors'],final_pictures=rows[-3:])
 print(name,json.dumps({k:measurements[name][k] for k in ['missing','pictures','before_end_update','composition_ms','last_sample']}))
(r/'measurements.json').write_text(json.dumps(measurements,indent=2)+'\n')
