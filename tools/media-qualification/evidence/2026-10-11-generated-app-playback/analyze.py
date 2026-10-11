from pathlib import Path
import json, math, sys
root=Path(__file__).parent

def distribution(values):
 values=sorted(values)
 if not values:return None
 return dict(count=len(values),p50=values[math.ceil(len(values)*.5)-1],p95=values[math.ceil(len(values)*.95)-1],max=values[-1])

results={}
for name in sys.argv[1:]:
 v=json.loads((root/name/'generated-playback.json').read_text())
 report=json.loads((root/name/'report.json').read_text())
 scenario=next(s for s in report['scenarios'] if s['name']=='generated-playback')
 events={}
 for step in scenario['steps']:
  for event in step['semantic'].get('stages',[]):
   ticket=event['ticket']
   if ticket is not None:events.setdefault(ticket,{})[event['stage']]=event
 samples=[]
 for picture in v['pictures']:
  stages=events[picture['ticket']]
  item=dict(picture)
  start=stages.get('picture_worker_started',{}).get('request_elapsed_ms')
  finish=stages.get('picture_worker_finished',{}).get('request_elapsed_ms')
  item['worker_ms']=None if start is None or finish is None else finish-start
  item['request_to_composed_ms']=stages.get('picture_composed',{}).get('request_elapsed_ms')
  samples.append(item)
 transitions=[s for i,s in enumerate(samples) if i and s['generated']!=samples[i-1]['generated']]
 coverage=set(p['frame'] for p in samples)
 assert v['missing_frames']==sorted(set(range(v['frames']))-coverage)
 results[name]=dict(frames=v['frames'],generated_frames=v['generated_frames'],missing_frames=v['missing_frames'],
  audio=v['audio'],ended=v['ended'],first_audio_ms=v['first_audio_ms'],elapsed_ms=v['elapsed_ms'],
  viewport=v.get('viewport_points'),basis=v.get('presentation_basis'),native_picture_counts=v.get('native_picture_counts'),
  transitions=transitions,
  worker_ms=distribution([p['worker_ms'] for p in samples if p['worker_ms'] is not None]),
  request_to_composed_ms=distribution([p['request_to_composed_ms'] for p in samples if p['request_to_composed_ms'] is not None]),
  sampled_clock_lag_frames=distribution([p['sampled_clock_lag_frames'] for p in samples if p['sampled_clock_lag_frames'] is not None]),
  checks=len(scenario['checks']),failures=[c['name'] for c in scenario['checks'] if not c['passed']],
  warnings=[f for f in scenario['findings'] if f['severity']=='warning'])
(root/'measurements.json').write_text(json.dumps(results,indent=2)+'\n')
for name,v in results.items():
 print(name,json.dumps({k:value for k,value in v.items() if k!='warnings'}))
