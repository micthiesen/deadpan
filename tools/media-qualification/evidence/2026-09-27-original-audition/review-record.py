from pathlib import Path
import json,hashlib,difflib
r=Path('/Users/michael/Code/deadpan');s=Path('/tmp/deadpan-original-audition-20260927');b=s/'before'
paths=[];diff=[]
for prefix in ('crates','native'):
 for p in sorted((r/prefix).rglob('*')):
  if not p.is_file() or p.suffix not in ('.rs','.toml'):continue
  rel=p.relative_to(r);old=b/rel
  if not old.exists() or old.read_bytes()!=p.read_bytes():
   paths.append(str(rel));diff.extend(difflib.unified_diff(old.read_text().splitlines(True) if old.exists() else [],p.read_text().splitlines(True),fromfile='before/'+str(rel),tofile=str(rel)))
record={'git_review_base':'origin/main','increment_base':'preserved before/ matching preceding verified moment-paste checkpoint','source_sha256':{p:hashlib.sha256((r/p).read_bytes()).hexdigest() for p in paths},'reviews':[
 {'agent':'audition_general_review','lens':'General correctness and interface integration','finding':'Zero-context selection at first/last picture used full A/V union endpoints and included lead/tail audio.','action':'Added separate measured selection-boundary mapping, Domain selection-window construction, and real offset/VFR regression. Ordinary Original cursor endpoints retain complete A/V union.','followup':'Reviewer confirmed resolved; paused-loop label also explicitly says Resume loop.'},
 {'agent':'audition_clock_review','lens':'Exact clocks, monotonic loops, canonical PCM, source admission/cache and bounded failure','findings':[]},
 {'agent':'audition_interaction_review','lens':'Keyboard/focus semantics and harness quality','findings':[]}],
 'limits':['Read-only static review, no native GUI or listening qualification.','Harness Original replay injects device updates; independent playback tests exercise real canonical PCM through a fake device queue.'],
 'discarded_findings':[], 'known_material_findings_remaining':[]}
(s/'review.json').write_text(json.dumps(record,indent=2)+'\n');(s/'increment.patch').write_text(''.join(diff));(s/'review-paths.json').write_text(json.dumps(paths,indent=2)+'\n')
print('Reviewed source files',len(paths))
