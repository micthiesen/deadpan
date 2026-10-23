from pathlib import Path
import json,hashlib,shutil,gzip
s=Path('/tmp/deadpan-first-pause-profile-2026-10-22')
e=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-first-pause-profile')
e.mkdir(exist_ok=True)
def sha(b):return hashlib.sha256(b).hexdigest()
def retain(path,rel):
 data=path.read_bytes();dest=e/rel
 if len(data)>200000 and path.suffix in ['.json','.stdout','.stderr']:
  dest=Path(str(dest)+'.gz');data=gzip.compress(data,mtime=0)
 dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
for path in sorted(s.iterdir()):
 if not path.is_file()or path.name.startswith('compact-'):continue
 if path.suffix not in ['.json','.py','.stdout','.stderr','.txt']:continue
 retain(path,Path('records')/path.name)
for name in ['attempt-1','deep-attempt-1','controlled','deep-controlled','overlays']:
 root=s/name
 for path in sorted(root.rglob('*')):
  if not path.is_file()or any(p.endswith('.deadpan')for p in path.relative_to(root).parts):continue
  retain(path,Path(name)/path.relative_to(root))
packages={}
for path in sorted(s.rglob('project.sqlite')):
 if not any(p.endswith('.deadpan')for p in path.parts):continue
 package=path.parent
 packages[str(package.relative_to(s))]={str(p.relative_to(package)):{'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())}for p in sorted(package.rglob('*'))if p.is_file()}
(e/'package-file-hashes.json').write_text(json.dumps(packages,indent=2)+'\n')
review=Path('/tmp/deadpan-neutral-ungroup-2026-10-22/next-performance-map')
for name in ['report.md','profiling-experiment.md','store-reuse-audit.md']:
 retain(review/name,Path('review')/name)
host=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-neutral-ungroup/host.json')
retain(host,Path('host.json'))
assert not list(e.rglob('*.rs'))
files={str(p.relative_to(e)):{'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())}for p in sorted(e.rglob('*'))if p.is_file()and p!=e/'inventory.json'}
(e/'inventory.json').write_text(json.dumps({'source_commit':'a18ff65443aa8fd1aa5ab63fa33678c9afd88220','files':files},indent=2)+'\n')
print(json.dumps({'payload_files':len(files),'payload_bytes':sum(r['bytes']for r in files.values()),'inventory_sha256':sha((e/'inventory.json').read_bytes())}))
