"""Recover first-run coverage from completed GPU tickets, after cleanup assertion failed."""
from pathlib import Path
import json,re
root=Path(__file__).parent
report=json.loads((root/'baseline-1080p/report.json').read_text())
scenario=next(s for s in report['scenarios'] if s['name']=='generated-loop')
pictures={};events={}
for step in scenario['steps']:
    displayed=step['semantic']['picture'].get('displayed')
    if displayed and 'Some(Generation' in displayed['ticket']:
        pictures[displayed['ticket']]=int(re.search(r'ProjectFrame\((\d+)\)',displayed['location'])[1])
    for event in step['semantic'].get('stages',[]):
        if event['ticket']:events.setdefault(event['ticket'],{})[event['stage']]=event
rows=[];previous=60;lap=0
for ticket,frame in pictures.items():
    if frame<previous:lap+=1
    previous=frame
    if 'picture_composed' in events.get(ticket,{}):
        stages=events[ticket]
        rows.append(dict(frame=frame,lap=lap,ticket=ticket,worker_ms=stages['picture_worker_finished']['request_elapsed_ms']-stages['picture_worker_started']['request_elapsed_ms']))
coverage={(r['lap'],r['frame']) for r in rows}
missing=[(lap,frame) for lap in range(30) for frame in range(60,121) if (lap,frame) not in coverage]
result=dict(completed_laps=lap,picture_count=len(rows),missing=missing,first_original=next(r for r in rows if r['frame']==90),failed_checks=[c['name'] for c in scenario['checks'] if not c['passed']])
assert not missing and lap==30 and len(rows)==1831
assert result['failed_checks']==['Settled picture matches the current session, revision, context and cursor']
(root/'initial-1080p-coverage.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
