from pathlib import Path
from datetime import datetime, timezone
import json
p=Path('/Users/michael/Code/deadpan/docs/progress.json')
d=json.loads(p.read_text())
d['updated_at']=datetime.now(timezone.utc).isoformat(timespec='seconds').replace('+00:00','Z')
d['focus']='Completing the remaining source and editing-operation qualification for DP-02 and DP-16.'
f=next(x for x in d['features'] if x['id']=='import')
f['status']='done'
f['note']='YouTube and local import are complete under the local verification scope. Managed/linked Originals, sound repair, missing-media export refusal and portable sound export are verified.'
d['history'].append({'date':'2026-10-11','percent':89,'complete_sections':7,'revision':'7fa6dd062dd75c94f94cef6cc2d306cc0f4c86e6','retrospective':False,'summary':'Closed DP-15: verified missing/corrupt sound export refusal, identical repair, portable linked sound export, explicit container-audio import and deduplication. All 360 picture comparisons, six zero-offset signal windows and 36 focused store tests pass.'})
p.write_text(json.dumps(d,indent=2)+'\n')
