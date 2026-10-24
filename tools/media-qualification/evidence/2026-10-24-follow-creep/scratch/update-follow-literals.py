from pathlib import Path
import re
roots=['crates/deadpan-core','crates/deadpan-plan','crates/deadpan-app','crates/deadpan-cli/src/framing']
paths=[p for root in roots for p in Path(root).rglob('*.rs')]
paths.append(Path('crates/deadpan-cli/src/framing.rs'))
changed=[]
for p in paths:
 if 'crates/deadpan-app/src/preview/harness/' in str(p): continue
 t=p.read_text(); inserts=[]
 for m in re.finditer(r'FramingValue::Follow\s*\{',t):
  start=m.end();depth=0;fieldstart=start;fields=[]
  for i in range(start,len(t)):
   ch=t[i]
   if ch in '{[(':depth+=1
   elif ch in '}])':
    if depth==0:
     fields.append(t[fieldstart:i]);end=i;break
    depth-=1
   elif ch==',' and depth==0:
    fields.append(t[fieldstart:i]);fieldstart=i+1
  else:raise AssertionError(str(p))
  if any('scale_transition' in f for f in fields):continue
  if any(f.lstrip().startswith('..') for f in fields):continue
  indent=re.search(r'(?m)^([ \t]*)\S',t[start:end])
  spacing=indent.group(1) if indent else '    '
  inserts.append((end,spacing+'scale_transition: None,\n'))
 for end,insert in reversed(inserts):
  # Closing indentation already precedes end; start on a fresh line.
  line=t.rfind('\n',0,end)+1
  closing=t[line:end]
  if closing.strip(): raise AssertionError((str(p),closing))
  t=t[:line]+insert+t[line:]
 if inserts:p.write_text(t);changed.append((str(p),len(inserts)))
print(changed)
p=Path('crates/deadpan-core/src/framing_zoom/tests.rs')
t=p.read_text();old="    Shot {\n        entry: None,"
assert t.count(old)==1
p.write_text(t.replace(old,"    Shot {\n        ordinary_source: false,\n        entry: None,"))
