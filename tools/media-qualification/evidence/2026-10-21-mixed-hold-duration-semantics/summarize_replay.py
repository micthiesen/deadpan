from pathlib import Path
import json
s=Path('/tmp/deadpan-mixed-hold-duration-next-2026-10-21')
summary={}
for mode in ['visual','performance']:
 r=json.loads((s/mode/'report.json').read_text())
 summary[mode]={'failed':r['failed'],'metadata':r['metadata'],'scenarios':[]}
 for sc in r['scenarios']:
  item={k:sc[k] for k in ['name','failed','findings']}
  item['checks']=len(sc['checks'])
  item['frames']=len(sc['steps'])
  item['timings']={t['name']:t['summary'] for t in sc['timings']}
  summary[mode]['scenarios'].append(item)
(s/'replay-result.json').write_text(json.dumps(summary,indent=2)+'\n')
r=json.loads((s/'visual/report.json').read_text())
sc=next(x for x in r['scenarios'] if x['name']=='scoped-plays')
captures=json.loads((s/'replay-notes.json').read_text())['captures']
selected=[]
for label in captures:
 matches=[step for step in sc['steps'] if step['input']==label and step['screenshot']]
 assert len(matches)==1,(label,len(matches))
 selected.append({'mode':'visual','file':matches[0]['screenshot'],'label':label,'frame':matches[0]['frame']})
(s/'selected-captures.json').write_text(json.dumps(selected,indent=2)+'\n')
print(json.dumps({mode:[{'name':v['name'],'failed':v['failed'],'checks':v['checks'],'frames':v['frames'],'metrics':{k:x for k,x in v['timings'].items() if k in ['input_to_state_ms','input_to_commit_ms','request_to_picture_complete_ms']}} for v in data['scenarios']] for mode,data in summary.items()},indent=2))
print(json.dumps(selected,indent=2))
