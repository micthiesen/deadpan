import json,re
from pathlib import Path
s=Path(__file__).resolve().parent
def collect(name):
 text=(s/(name+'.log')).read_text()
 rows=[]
 for chunk in re.split(r'\n\s+(?:Running |Doc-tests )',text)[1:]:
  heading=chunk.splitlines()[0]
  result=re.search(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',chunk)
  if result:rows.append(dict(target=heading,result=result[1],passed=int(result[2]),failed=int(result[3]),ignored=int(result[4])))
 return rows
initial=collect('full-tests-initial')
# App/audio completed before the stale core adversarial fixture stopped Cargo.
selected=[]
for row in initial:
 if 'deadpan_core-' in row['target']:break
 selected.append(row)
remaining=collect('full-tests-remaining-fixed')
report=dict(initial_completed=selected,remaining=remaining,passed=sum(x['passed'] for x in selected+remaining),failed=sum(x['failed'] for x in selected+remaining),ignored=sum(x['ignored'] for x in selected+remaining))
(s/'test-counts.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if isinstance(v,int)}))
