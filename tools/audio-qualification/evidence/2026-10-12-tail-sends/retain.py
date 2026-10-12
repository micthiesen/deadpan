"""Retain checks, actual rendered media, source identities and final UI evidence."""
import gzip,hashlib,io,json,shutil,subprocess,tarfile
from pathlib import Path
scratch=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
out=repo/'tools/audio-qualification/evidence/2026-10-12-tail-sends'
out.mkdir(parents=True,exist_ok=True)
def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def write(name,data):(out/name).write_text(json.dumps(data,indent=2)+'\n')
records=[]
for receipt in sorted(scratch.glob('*.json')):
 data=json.loads(receipt.read_text())
 if not isinstance(data,dict) or 'log_sha256' not in data:continue
 log=scratch/(data['name']+'.log')
 assert sha(log)==data['log_sha256'],log
 records.append(data)
for log in sorted(scratch.glob('*.log')):
 (out/(log.name+'.gz')).write_bytes(gzip.compress(log.read_bytes(),mtime=0))
write('commands.json',records)
base='b3f961658652b1282001d8dcb87121d20d769c5e'
changed=subprocess.check_output(['git','diff','--name-only',base,'--','crates'],cwd=repo,text=True).splitlines()
new=subprocess.check_output(['git','ls-files','--others','--exclude-standard','--','crates'],cwd=repo,text=True).splitlines()
sources=sorted(set(changed+new+['Cargo.toml','Cargo.lock','rust-toolchain.toml','native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav']))
write('identity.json',{'base_commit':base,'sources':{name:sha(repo/name) for name in sources},'binaries':{'target/release/deadpan-app':sha(repo/'target/release/deadpan-app'),'target/release/deadpan-cli':sha(repo/'target/release/deadpan-cli')}})
patch=subprocess.check_output(['git','diff',base,'--',*changed],cwd=repo)
for name in new:patch+=subprocess.run(['git','diff','--no-index','--','/dev/null',name],cwd=repo,capture_output=True).stdout
(out/'implementation.patch').write_bytes(patch)
for name in ['run.py','finish-checks.py','post-checks.py','replays.py','retain.py','check-evidence.py','README.md','review.md','default-only.json','wav-oracle.json']:
 shutil.copyfile(scratch/name,out/name)
write('environment.json',dict(machine='Apple M5 Max',physical_cores=18,memory_bytes=137438953472,architecture='arm64',macos='26.5.2 (25F84)',rust='rustc 1.97.1 (8bab26f4f 2026-07-14)',ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',agents='primary agent only',live_gui=False,device_run=False))
export=json.loads((scratch/'export-results.json').read_text())
assert export['report']['passed'] and not export['report']['failures']
write('export-results.json',export)
movie=Path(export['movie']);shutil.copyfile(movie,out/'selected-tail-send.mp4')
receipts=list(movie.parent.glob('deadpan-render-*.json'));assert len(receipts)==1
shutil.copyfile(receipts[0],out/'render-receipt.json')
package=Path(export['package']);wal=package/'project.sqlite-wal'
assert not wal.exists() or wal.stat().st_size==0
stream=io.BytesIO()
with tarfile.open(fileobj=stream,mode='w') as archive:
 for path in sorted(package.rglob('*')):
  if path.is_file() and not path.name.endswith(('-shm','-wal','.lock')):
   item=archive.gettarinfo(str(path),arcname='selected-tail-send.deadpan/'+str(path.relative_to(package)))
   item.uid=item.gid=item.mtime=0;item.uname=item.gname=''
   with path.open('rb') as f:archive.addfile(item,f)
(out/'selected-tail-send.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(),mtime=0))
for name in ['replay1','replay2','replay-final','hold-effects','audio-treatments','equalizer']:
 path=scratch/name/'report.json'
 (out/(name+'-report.json.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
r=json.loads((scratch/'replay-final/report.json').read_text())
s=next(s for s in r['scenarios'] if s['name']=='tail-sends')
for label,name in [('Selected beat sends','beat-sends.png'),('Selected sends into a tail pause','tail-input.png'),('Placed sound sends','sound-sends.png')]:
 step=next(t for t in s['steps'] if t['input']==label)
 shutil.copyfile(scratch/'replay-final'/step['screenshot'],out/name)
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files in',out)
