from pathlib import Path
import json,gzip,hashlib
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
dest=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-compact-frozen-edges/checkpoint')
base=json.loads(Path('/tmp/deadpan-first-pause-profile-2026-10-22/source-identities.json').read_text())
phases=['candidate-core','candidate-all-core','candidate-all-core-corrected','candidate-all-core-final','baseline-core','candidate-broad','candidate-broad-resumed','candidate-broad-final','candidate-ui','candidate-strict','debug-media']
def put(name,data):
 if len(data)>200000 and name.endswith('.json'):
  name+='.gz';data=gzip.compress(data,mtime=0)
 path=dest/name;path.parent.mkdir(parents=True,exist_ok=True)
 if path.exists():assert path.read_bytes()==data,path
 path.write_bytes(data)
for phase in phases:
 ids=json.loads((s/(phase+'-source-identities.json')).read_text())
 for name,h in ids.items():
  if base.get(name)==h:continue
  overlay=s/(phase+'-overlays')/(name+'.txt')
  if not overlay.is_file():overlay=s/(phase+'-source')/name
  data=overlay.read_bytes();assert hashlib.sha256(data).hexdigest()==h,(phase,name)
  put(phase+'/source-overlays/'+name+'.txt',data)
 for suffix in ['source-identities.json','inputs.json','lock.json','build.log','build-receipt.json','run.log','run-receipt.json','check.log','check-receipt.json','format.log','format-receipt.json','binaries.json','helpers.json','result.json','archive-reuse.json','neutral.log','neutral-receipt.json','neutral-fixture.json']:
  path=s/(phase+'-'+suffix)
  if path.is_file():put(phase+'/'+path.name,path.read_bytes())
for name in ['reviews.json','host.json','baseline-copy-receipt.json','baseline-copy-initial-failure.json','baseline-witness-verification.json','fixture-correction-attribution.json','store-fixture-correction-attribution.json','broad-resume-attribution.json','qualify.py','resume_archived_broad.py','guard.py','freeze.py','retain_checkpoint.py']:
 put('procedure/'+name,(s/name).read_bytes())
for path in sorted((s/'historical-codec-probe').iterdir()):
 if path.suffix not in ['.json','.rs','.stdout','.stderr']:continue
 put('historical-codec-probe/'+path.name+('.txt'if path.suffix=='.rs'else''),path.read_bytes())
metadata={'source_base':'a18ff65443aa8fd1aa5ab63fa33678c9afd88220','qualification_in_progress':True,'completed_phases':phases,'pending':['release-media','release-native','portable extraction checks','strengthened historical/asymmetric fixture checks','current-format performance']}
put('status.json',(json.dumps(metadata,indent=2)+'\n').encode())
assert not list(dest.rglob('*.rs'))
files={str(p.relative_to(dest)):{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}for p in sorted(dest.rglob('*'))if p.is_file()and p!=dest/'inventory.json'}
put('inventory.json',(json.dumps({'files':files},indent=2)+'\n').encode())
print(json.dumps({'payload_files':len(files),'payload_bytes':sum(r['bytes']for r in files.values()),'inventory_sha256':hashlib.sha256((dest/'inventory.json').read_bytes()).hexdigest()}))
