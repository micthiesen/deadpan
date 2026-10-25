import json,math
from pathlib import Path
P=Path(__file__).resolve().parent;A=P.parent/'native-assembly/first'
out=[]
def compare(a,b,path='',floats=None):
 if floats is None:floats=[]
 assert type(a)==type(b),(path,type(a),type(b))
 if isinstance(a,dict):
  assert a.keys()==b.keys(),path
  for k in a:compare(a[k],b[k],path+'/'+k,floats)
 elif isinstance(a,list):
  assert len(a)==len(b),path
  for i,(x,y) in enumerate(zip(a,b)):compare(x,y,path+'/'+str(i),floats)
 elif isinstance(a,float):
  assert math.isfinite(a) and math.isfinite(b),path
  floats.append((abs(a-b),path))
 else:assert a==b,(path,a,b)
 return floats
for case in sorted((P/'goldens').iterdir()):
 new=json.loads((A/(case.name+'-nonfolded-output.json')).read_text())
 old={k:json.loads((case/v).read_text()) for k,v in [('proposal','raw-suggestion.json'),('guard','guard-output.json'),('filtered','filtered-suggestion.json')]}
 diffs=compare(new,old)
 out.append({'case':case.name,'all_nonfloat_fields_identical':True,'float_fields_compared':len(diffs),
             'maximum_absolute_float_difference':max((x for x,_ in diffs),default=0),
             'largest_difference_field':max(diffs,default=(0,''))[1]})
result=dict(cases=out,full_guard_paths_residuals_spans_choices_statuses_identical=True,
 meaning='Compare every JSON key, array length and non-float field including every path, edit operation, residual qualification and refusal. Float diagnostics reported, never rounded for comparison.')
(P/'export-parity-audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result),flush=True)
