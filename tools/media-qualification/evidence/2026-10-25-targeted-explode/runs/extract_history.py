#!/usr/bin/env python3
"""Read-only identity-bound extraction of the retained native history request."""
from pathlib import Path
import hashlib, json, sqlite3

ROOT = Path('/tmp/deadpan-targeted-explode-2026-10-25/native-second-retained/projects/zoom/Documents/Deadpan/cfr-bframes.deadpan')
DB = ROOT / 'project.sqlite'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

conn = sqlite3.connect(f'file:{DB}?mode=ro', uri=True)
conn.row_factory = sqlite3.Row
check = conn.execute('PRAGMA quick_check').fetchone()[0]
row = None
for candidate in conn.execute('SELECT id,parent_id,revision_id,request FROM history ORDER BY id DESC'):
    request = json.loads(candidate['request'])
    if request.get('command',{}).get('command') == 'compound' and 'explode' in candidate['request']:
        row = dict(candidate)
        row['request'] = request
        break
if row is None: raise RuntimeError('no compound Explode request found')
receipt = conn.execute('SELECT revisions,head,validator FROM history_receipt WHERE singleton=1').fetchone()
result = {
  'extraction':'read-only SQLite mode=ro; WAL visible; no product writes',
  'database':{'path':str(DB),'bytes':DB.stat().st_size,'sha256':sha(DB)},
  'wal':{'path':str(ROOT/'project.sqlite-wal'),'bytes':(ROOT/'project.sqlite-wal').stat().st_size,'sha256':sha(ROOT/'project.sqlite-wal')},
  'manifest':{'path':str(ROOT/'manifest.json'),'bytes':(ROOT/'manifest.json').stat().st_size,'sha256':sha(ROOT/'manifest.json')},
  'original':{'path':str(ROOT/'Media/Originals/blake3-16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1'),'bytes':(ROOT/'Media/Originals/blake3-16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1').stat().st_size,'sha256':sha(ROOT/'Media/Originals/blake3-16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1')},
  'integrity':{'quick_check':check,'history_rows':conn.execute('SELECT count(*) FROM history').fetchone()[0],
               'history_receipt':dict(receipt)},
  'history_request_row':row,
  'asserted_identity':{'outer_revision':'c678df92-b263-4503-9221-a8b48f6f492a',
                       'leaf_revision_and_timing_allocation':'66d93d0c-9ea9-4bad-87db-fa9026a42346'}
}
conn.close()
out=Path('/tmp/deadpan-targeted-explode-2026-10-25/native-history-request.json')
out.write_text(json.dumps(result,sort_keys=True,indent=2)+'\n')
print(json.dumps({'path':str(out),'bytes':out.stat().st_size,'sha256':sha(out),'quick_check':check,
                  'history_rows':result['integrity']['history_rows'],'row':row['id'],
                  'outer':row['revision_id'],'leaf':row['request']['command']['transaction']['steps'][0]['edit']['new_revision']}))
