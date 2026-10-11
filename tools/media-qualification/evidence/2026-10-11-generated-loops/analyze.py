from pathlib import Path
import json,math,re,sys
root=Path(__file__).parent
def distribution(values):
    values=sorted(values)
    return dict(count=len(values),p95=values[math.ceil(len(values)*.95)-1],max=values[-1]) if values else None
results={}
for name in sys.argv[1:]:
    report=json.loads((root/name/'report.json').read_text())
    scenario=next(s for s in report['scenarios'] if s['name']=='generated-loop')
    path=root/name/'generated-loop.json'
    if not path.exists():
        print(name,json.dumps(scenario['findings']));continue
    value=json.loads(path.read_text())
    events={}
    for step in scenario['steps']:
        for event in step['semantic'].get('stages',[]):
            if event['ticket'] is not None: events.setdefault(event['ticket'],{})[event['stage']]=event
    rows=[]
    for picture in value['pictures']:
        stages=events.get(picture['ticket'],{})
        begin=stages.get('picture_worker_started',{}).get('request_elapsed_ms')
        end=stages.get('picture_worker_finished',{}).get('request_elapsed_ms')
        rows.append(dict(picture,worker_ms=None if begin is None or end is None else end-begin,composition_ms=stages.get('picture_composed',{}).get('request_elapsed_ms')))
    boundaries=[row for i,row in enumerate(rows) if row['frame']==value['window_frames'][0] or i and row['generated']!=rows[i-1]['generated']]
    groups={str(frame):[r for r in boundaries if r['frame']==frame] for frame in sorted({r['frame'] for r in boundaries})}
    log=(root/(name+'.log')).read_text()
    footprint=re.search(r'(\d+)\s+peak memory footprint',log)
    results[name]=dict(window=value['window_frames'],laps=value['expected_laps'],ended=value['ended'],
        missing=value['missing_frames'],audio=value['audio'],native=value['native_picture_counts'],
        errors=value['errors'],first_audio_ms=value['first_audio_ms'],elapsed_ms=value['elapsed_ms'],
        picture_count=len(rows),worker_ms=distribution([r['worker_ms'] for r in rows if r['worker_ms'] is not None]),
        composition_ms=distribution([r['composition_ms'] for r in rows if r['composition_ms'] is not None]),
        clock_lag=distribution([r['sampled_clock_lag_frames'] for r in rows if r['sampled_clock_lag_frames'] is not None]),
        footprint_bytes=None if footprint is None else int(footprint[1]),
        boundary_groups={frame:dict(count=len(items),first=items[0],worker_ms=distribution([r['worker_ms'] for r in items if r['worker_ms'] is not None]),composition_ms=distribution([r['composition_ms'] for r in items if r['composition_ms'] is not None])) for frame,items in groups.items()},
        boundaries=boundaries,failures=[c['name'] for c in scenario['checks'] if not c['passed']])
    print(name,json.dumps({k:v for k,v in results[name].items() if k!='boundaries'}))
(root/'measurements.json').write_text(json.dumps(results,indent=2)+'\n')
