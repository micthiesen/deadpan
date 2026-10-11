from pathlib import Path
import hashlib,json,gzip,subprocess,re
repo=Path('/Users/michael/Code/deadpan');scratch=Path(__file__).parent
root=repo/'tools/media-qualification/evidence/2026-10-11-real-generated-export';root.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
def cmd(args):return subprocess.check_output(args,cwd=repo,text=True).strip()
previous=json.loads(Path('/tmp/deadpan-generated-playback-performance-20261011/identity.json').read_text())
production=['crates/deadpan-cli/src','crates/deadpan-render','crates/deadpan-store','crates/deadpan-core','crates/deadpan-audio','crates/deadpan-media','native']
assert cmd(['git','diff','--name-only','29ad60a9','HEAD','--',*production])==''
source=['crates/deadpan-cli/src/export_verification.rs','crates/deadpan-cli/src/export_verification/audio_check.rs','crates/deadpan-cli/src/export_picture.rs','crates/deadpan-cli/tests/preview_export/recipes.rs','crates/deadpan-core/src/semantic/program.rs']
identity=dict(revision=cmd(['git','rev-parse','HEAD']),cli_production_unchanged_since='29ad60a9',binary_dir=previous['binary_dir'],binary_sha256=previous['binary_sha256'],libraries_sha256=previous['libraries_sha256'],source_sha256={p:sha((repo/p).read_bytes()) for p in source},rust=cmd(['rustc','--version']),os=cmd(['sw_vers']),hardware=previous['hardware'],fixture_ffmpeg=cmd(['which','ffmpeg']))
identity['fixture_ffmpeg_sha256']=sha(Path(identity['fixture_ffmpeg']).read_bytes())
(scratch/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
pins={}
for name in ['4k','chirp-4k','chirp-v2-4k','chirp-v2-1080p']:
 for folder in [scratch/(name+'.deadpan'),scratch/(name+'-exports')]:
  for p in sorted(folder.rglob('*')):
   if p.is_file() and p.name not in {'.writer.lock','.host.json'} and not p.name.endswith(('-shm','-wal')):
    pins[str(p.relative_to(scratch))]=dict(bytes=p.stat().st_size,sha256=sha(p.read_bytes()))
 p=scratch/(name+'.mp4')
 if p.is_file():pins[p.name]=dict(bytes=p.stat().st_size,sha256=sha(p.read_bytes()))
(scratch/'artifact-pins.json').write_text(json.dumps(pins,indent=2)+'\n')
manifest=[]
def add(name,data,compress=False):
 payload=gzip.compress(data,mtime=0) if compress else data
 if compress:name+='.gz'
 target=root/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(payload)
 manifest.append(dict(path=name,bytes=len(payload),sha256=sha(payload),original_sha256=sha(data)))
for p in sorted(scratch.iterdir()):
 if p.is_file() and p.suffix in {'.json','.py','.stdout','.stderr','.log','.txt'}:
  add(p.name,p.read_bytes(),p.suffix in {'.stdout','.stderr','.log','.txt'} or p.name.endswith('-verification.json'))
for name in source:add('source/'+name,(repo/name).read_bytes(),True)
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(row['bytes'] for row in manifest))))
