from pathlib import Path
from datetime import datetime,timezone
import json
repo=Path('/Users/michael/Code/deadpan')
p=repo/'docs/REQUIREMENTS.md';text=p.read_text();row=next(r for r in text.splitlines() if r.startswith('| DP-02 |'))
assert text.count(row)==1 and '| Partial |' in row
old='Remaining source policies and the full source-format, VFR and creative-operation playback/export matrix. The measured 10,000-command workload uses six live nodes; it does not qualify 10,000-node performance.'
assert old in row
new='[Final exact-timing acceptance](qualification/exact-timing-acceptance-2026-10-11.md) reconciles every §4 obligation with the implemented clock contract and accumulated media evidence. The added combined-edit matrix passes 40 exports across 20 H.264/HEVC/VP9/AV1/ProRes, VFR, HDR and interlaced fixtures: 1,040 independently expected pictures, 36 zero-offset signal windows and exact terminal sample counts. Complete under §29.1. New operations still need timing evidence; unfinished effects, renderer/device qualification and performance remain with DP-07/DP-16/DP-17/DP-24. The measured 10,000-command workload uses six live nodes and does not qualify 10,000-node performance.'
changed=row.replace('| Partial |','| Complete |',1).replace(old,new)
p.write_text(text.replace(row,changed))
p=repo/'docs/progress.json';d=json.loads(p.read_text())
d['updated_at']=datetime.now(timezone.utc).isoformat(timespec='seconds').replace('+00:00','Z')
d['focus']='Completing the remaining shared renderer and playback requirements in DP-16.'
f=next(x for x in d['features'] if x['id']=='render')
f['note']='Exact timing is complete under the local verification scope. Forty combined-edit exports across 20 source fixtures pass independent picture clocks and zero-offset audio; renderer, effects and output qualification remain.'
d['history'].append({'date':'2026-10-11','percent':89,'complete_sections':8,'revision':'ad6709f2bc2bf8e13f29ea5b1368d95540094a93','retrospective':False,'summary':'Closed DP-02: added combined source-format editing tests and reconciled the exact-time contract. Forty release exports pass 1,040 independent picture identities, 36 zero-offset signal windows and exact terminal sample counts; strict Clippy passes.'})
p.write_text(json.dumps(d,indent=2,ensure_ascii=False)+'\n')
