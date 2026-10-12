"""Retain serial checks and actual event-return export/replay evidence."""
import gzip,hashlib,io,json,shutil,subprocess,tarfile
from pathlib import Path
s=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
out=repo/'tools/audio-qualification/evidence/2026-10-12-event-returns'
out.mkdir(parents=True,exist_ok=True)
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def write(n,v):(out/n).write_text(json.dumps(v,indent=2)+'\n')
records=[]
for p in sorted(s.glob('*.json')):
 r=json.loads(p.read_text())
 if not isinstance(r,dict) or 'log_sha256' not in r:continue
 assert sha(s/(r['name']+'.log'))==r['log_sha256']
 records.append(r)
for p in sorted(s.glob('*.log')):(out/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
write('commands.json',records)
base='8db66bed2336ef6f3061832972d902032f0da713'
changed=subprocess.check_output(['git','diff','--name-only',base,'--','crates'],cwd=repo,text=True).splitlines()
new=subprocess.check_output(['git','ls-files','--others','--exclude-standard','--','crates'],cwd=repo,text=True).splitlines()
sources=sorted(set(changed+new+['Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/deadpan-audio/src/finite_wet.rs','crates/deadpan-audio/src/retained_sound_input.rs','native/deadpan-dsp/src/causal_filter.cpp','native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']))
write('identity.json',{'base_commit':base,'sources':{n:sha(repo/n) for n in sources},'binaries':{n:sha(repo/n) for n in ['target/release/deadpan-app','target/release/deadpan-cli']}})
patch=subprocess.check_output(['git','diff',base,'--',*changed],cwd=repo)
for n in new:patch+=subprocess.run(['git','diff','--no-index','--','/dev/null',n],cwd=repo,capture_output=True).stdout
(out/'implementation.patch').write_bytes(patch)
for n in ['run.py','final_checks.py','source_checks.py','replay_fix_checks.py','debug_replays.py','replays.py','retain.py','check-evidence.py','README.md','review.md','review_diff.py','diff-review.json']:
 shutil.copyfile(s/n,out/n)
write('environment.json',dict(machine='Apple M5 Max',physical_cores=18,memory_bytes=137438953472,architecture='arm64',macos='26.5.2 (25F84)',rust='rustc 1.97.1 (8bab26f4f 2026-07-14)',ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',agents='primary agent only',live_gui=False,device_run=False))
e=json.loads((s/'export-results.json').read_text());assert e['report']['passed'] and not e['report']['failures']
write('export-results.json',e)
m=Path(e['movie']);shutil.copyfile(m,out/'continuous-event-return.mp4')
r=list(m.parent.glob('deadpan-render-*.json'));assert len(r)==1
shutil.copyfile(r[0],out/'render-receipt.json')
p=Path(e['package']);wal=p/'project.sqlite-wal';assert not wal.exists() or wal.stat().st_size==0
stream=io.BytesIO()
with tarfile.open(fileobj=stream,mode='w') as archive:
 for f in sorted(p.rglob('*')):
  if f.is_file() and not f.name.endswith(('-shm','-wal','.lock')):
   item=archive.gettarinfo(str(f),arcname='continuous-event-return.deadpan/'+str(f.relative_to(p)))
   item.uid=item.gid=item.mtime=0;item.uname=item.gname=''
   with f.open('rb') as data:archive.addfile(item,data)
(out/'continuous-event-return.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(),mtime=0))
for n in ['wet-replay','tail-replay','sound-replay','audio-replay','eq-replay']:
 p=s/n/'report.json';(out/(n+'-report.json.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
p=s/'wet-replay-initial/report.json';(out/'wet-replay-initial-report.json.gz').write_bytes(gzip.compress(p.read_bytes(),mtime=0))
shutil.copyfile(s/'wet-replay-initial/wet-returns-122.png',out/'initial-wet-failure.png')
p=s/'sound-replay-initial/report.json';(out/'sound-replay-initial-report.json.gz').write_bytes(gzip.compress(p.read_bytes(),mtime=0))
shutil.copyfile(s/'sound-replay-initial/sound-placement-060.png',out/'initial-sound-failure.png')
shutil.copyfile(s/'wet-replay-heading/wet-returns-122.png',out/'inspector-heading-failure.png')
shutil.copyfile(s/'sound-replay-role/sound-placement-063.png',out/'catalog-role-failure.png')
for n in ['wet-replay-heading','sound-replay-role','wet-debug','sound-debug']:
 p=s/n/'report.json';(out/(n+'-report.json.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
r=json.loads((s/'wet-replay/report.json').read_text());scenario=next(x for x in r['scenarios'] if x['name']=='wet-returns')
for label,n in [('Placed sound continuous returns','event-returns-minimum.png'),('Placed sound continuous returns at default size','event-returns.png')]:
 step=next(x for x in scenario['steps'] if x['input']==label)
 shutil.copyfile(s/'wet-replay'/step['screenshot'],out/n)
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files in',out)
