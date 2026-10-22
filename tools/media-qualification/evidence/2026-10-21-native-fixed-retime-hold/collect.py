from pathlib import Path
import gzip,hashlib,json,shutil
s=Path('/tmp/deadpan-native-fixed-retime-hold-2026-10-21')
out=Path('tools/media-qualification/evidence/2026-10-21-native-fixed-retime-hold')
out.mkdir(parents=True,exist_ok=True)
def copy(src,name=None):
 name=name or src.name
 dst=out/name
 if src.suffix in ['.log','.rs','.txt'] or (src.suffix != '.png' and src.stat().st_size>100000):
  dst=out/(name+'.gz')
  dst.write_bytes(gzip.compress(src.read_bytes(),mtime=0))
 else: shutil.copy2(src,dst)
for p in s.iterdir():
 if p.is_file() and p.suffix in ['.log','.json','.sh','.py','.txt','.md','.rs']:
  copy(p)
for section in ['baseline-source','final-source']:
 for p in (s/section).rglob('*.rs'):
  copy(p,section+'__'+str(p.relative_to(s/section)).replace('/','__'))
for name in ['core/first-compile-tests.rs','core/design.md','core/source-freeze.json']:
 p=s/name
 if p.exists(): copy(p,name.replace('/','__'))
for mode in ['visual-first','visual-second','visual','performance']:
 directory=s/mode
 if directory.exists():
  for p in directory.rglob('*.json'): copy(p,mode+'__'+str(p.relative_to(directory)).replace('/','__'))
for name in ['scoped-plays-139.png','scoped-plays-141.png','scoped-plays-142.png','scoped-plays-146.png','scoped-plays-147.png']:
 copy(s/'visual'/name,'visual-'+name)
copy(s/'visual-first/scoped-plays-141.png','visual-first-scoped-plays-141.png')
inventory=[]
for p in sorted(out.iterdir()):
 if p.name=='inventory.json':continue
 inventory.append({'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
(out/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
for entry in json.loads((out/'inventory.json').read_text()):
 p=out/entry['file']
 assert p.stat().st_size==entry['bytes']
 assert hashlib.sha256(p.read_bytes()).hexdigest()==entry['sha256'],p
print(len(inventory),'entries',sum(e['bytes'] for e in inventory),'bytes; all rehashed')
