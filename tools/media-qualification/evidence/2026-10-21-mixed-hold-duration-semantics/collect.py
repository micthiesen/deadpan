from pathlib import Path
import gzip,hashlib,json,shutil
s=Path('/tmp/deadpan-mixed-hold-duration-next-2026-10-21')
out=Path('tools/media-qualification/evidence/2026-10-21-mixed-hold-duration-semantics')
out.mkdir(parents=True,exist_ok=True)
def copy(src,name=None):
 name=name or src.name
 dst=out/name
 if src.suffix in ['.log','.rs','.txt'] or (src.suffix!='.png' and src.stat().st_size>100000):
  dst=out/(name+'.gz')
  dst.write_bytes(gzip.compress(src.read_bytes(),mtime=0))
 else: shutil.copy2(src,dst)
for p in s.iterdir():
 if p.is_file() and p.suffix in ['.log','.json','.sh','.py','.txt','.md','.rs']:
  copy(p)
snapshots={}
for directory in ['baseline-source','first-runtime-source']+sorted(p.name for p in s.iterdir() if p.is_dir() and p.name.startswith('final-source')):
 rows=[]
 for p in sorted((s/directory).rglob('*.rs')):
  digest=hashlib.sha256(p.read_bytes()).hexdigest()
  blob='source-'+digest+'.rs'
  if not (out/(blob+'.gz')).exists(): copy(p,blob)
  rows.append({'path':str(p.relative_to(s/directory)),'sha256':digest,'bytes':p.stat().st_size,'blob':blob+'.gz'})
 snapshots[directory]=rows
(out/'source-snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
for mode in sorted(p.name for p in s.iterdir() if p.is_dir() and (p.name.startswith('visual') or p.name=='performance')):
 directory=s/mode
 if directory.exists():
  for p in directory.rglob('*.json'):
   copy(p,mode+'__'+str(p.relative_to(directory)).replace('/','__'))
selected=json.loads((s/'selected-captures.json').read_text())
for capture in selected:
 copy(s/capture['mode']/capture['file'],capture['mode']+'-'+capture['file'])
inventory=[]
for p in sorted(out.iterdir()):
 if p.name=='inventory.json': continue
 inventory.append({'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
(out/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
for entry in json.loads((out/'inventory.json').read_text()):
 p=out/entry['file']
 assert p.stat().st_size==entry['bytes']
 assert hashlib.sha256(p.read_bytes()).hexdigest()==entry['sha256'],p
print(len(inventory),'entries',sum(e['bytes'] for e in inventory),'bytes; all rehashed')
