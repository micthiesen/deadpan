from pathlib import Path
import gzip, hashlib, json, subprocess
scratch=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-app-playback'
root.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
identity=json.loads((scratch/'identity.json').read_text())
(scratch/'source.patch').write_bytes(subprocess.check_output(['git','diff','--',*identity['source_sha256']],cwd=repo))
manifest=[]
def add(name,data,compress=False):
 output=gzip.compress(data,mtime=0) if compress else data
 name+= '.gz' if compress else ''
 target=root/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(output)
 manifest.append(dict(path=name,bytes=len(output),sha256=sha(output),original_sha256=sha(data)))
for path in sorted(scratch.iterdir()):
 if path.is_file() and path.suffix in {'.py','.json','.log','.patch'}:
  add(path.name,path.read_bytes(),path.suffix in {'.log','.patch'})
for name in ['smoke-4k','real-4k','real-1080p','control-original','control-sound','verified-4k','verified-1080p']:
 for filename in ['report.json','generated-playback.json']:
  path=scratch/name/filename
  if path.is_file():add(name+'/'+filename,path.read_bytes(),True)
for label in ['telemetry-old','telemetry-fixed']:
 for filename in ['test.rs','telemetry.rs']:
  path=scratch/label/filename
  add(label+'/'+filename,path.read_bytes(),True)
for name in identity['source_sha256']:
 add('source/'+name,(repo/name).read_bytes(),True)
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(row['bytes'] for row in manifest))))
