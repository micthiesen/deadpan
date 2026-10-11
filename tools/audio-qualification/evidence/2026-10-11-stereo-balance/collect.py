from pathlib import Path
import gzip,hashlib,json,shutil,subprocess
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
e=repo/'tools/audio-qualification/evidence/2026-10-11-stereo-balance'
e.mkdir(parents=True,exist_ok=True)
for p in r.iterdir():
 if p.is_file() and p.suffix in ['.py','.json','.log','.txt']:
  if p.suffix=='.log':
   (e/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
  else:shutil.copy2(p,e/p.name)
for scenario in ['audio-treatments','gain']:
 p=r/scenario/'report.json'
 (e/(scenario+'-report.json.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
info=json.loads((r/'export-report.json').read_text())
shutil.copy2(info['movie'],e/'stereo-balance.mp4')
(e/'decoded-aac.f32le.gz').write_bytes(gzip.compress((r/'decoded-aac.f32le').read_bytes(),mtime=0))
selected=json.loads((r/'selected-captures.json').read_text())
(e/'captures').mkdir(exist_ok=True)
for dest,source in selected.items():shutil.copy2(r/source,e/'captures'/dest)
manifest={str(p.relative_to(e)):{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size} for p in sorted(e.rglob('*')) if p.is_file() and p.name!='manifest.json'}
(e/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'files':len(manifest),'bytes':sum(v['bytes'] for v in manifest.values()),'path':str(e)}))
