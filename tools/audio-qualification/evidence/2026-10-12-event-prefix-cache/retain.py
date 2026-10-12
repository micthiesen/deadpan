import gzip,hashlib,io,json,re,shutil,subprocess,tarfile
from pathlib import Path
s=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
out=repo/'tools/audio-qualification/evidence/2026-10-12-event-prefix-cache'
out.mkdir(parents=True,exist_ok=True)
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def write(n,v):(out/n).write_text(json.dumps(v,indent=2)+'\n')
records=[]
for p in sorted(s.glob('*.json')):
 r=json.loads(p.read_text())
 if isinstance(r,dict) and 'log_sha256' in r:
  assert sha(s/(r['name']+'.log'))==r['log_sha256']
  records.append(r)
write('commands.json',records)
for p in sorted(s.glob('*.log')):(out/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
base=subprocess.check_output(['git','rev-parse','6684f981'],cwd=repo,text=True).strip()
changed=subprocess.check_output(['git','diff','--name-only',base,'--','crates'],cwd=repo,text=True).splitlines()
new=subprocess.check_output(['git','ls-files','--others','--exclude-standard','--','crates'],cwd=repo,text=True).splitlines()
sources=sorted(set(changed+new+['Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/deadpan-audio/src/stages.rs','crates/deadpan-audio/src/shared_eq.rs','crates/deadpan-audio/src/limited.rs','native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']))
write('identity.json',dict(base_commit=base,sources={n:sha(repo/n) for n in sources},binaries={n:sha(repo/n) for n in ['target/release/deadpan-app','target/release/deadpan-cli']}))
patch=subprocess.check_output(['git','diff',base,'--','crates'],cwd=repo)
for n in new:patch+=subprocess.run(['git','diff','--no-index','--','/dev/null',n],cwd=repo,capture_output=True).stdout
(out/'implementation.patch').write_bytes(patch)
for n in ['run.py','checks.py','retain.py','check-evidence.py','README.md','review.md']:shutil.copyfile(s/n,out/n)
results=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (s/'audio-playback.log').read_text())
assert results and not any(int(row[1]) for row in results)
write('test-counts.json',dict(passed=sum(int(x[0]) for x in results),ignored=sum(int(x[2]) for x in results),failed=0,result_groups=len(results)))
write('environment.json',dict(machine='Apple M5 Max',physical_cores=18,memory_bytes=137438953472,macos='26.5.2 (25F84)',rust='1.97.1',agents='primary only',live_gui=False,physical_device=False))
e=json.loads((s/'export-results.json').read_text());assert e['report']['passed'] and not e['report']['failures']
write('export-results.json',e)
m=Path(e['movie']);shutil.copyfile(m,out/'treated-event.mp4')
r=list(m.parent.glob('deadpan-render-*.json'));assert len(r)==1
shutil.copyfile(r[0],out/'render-receipt.json')
p=Path(e['package']);wal=p/'project.sqlite-wal';assert not wal.exists() or wal.stat().st_size==0
stream=io.BytesIO()
with tarfile.open(fileobj=stream,mode='w') as archive:
 for f in sorted(p.rglob('*')):
  if f.is_file() and not f.name.endswith(('-shm','-wal','.lock')):
   item=archive.gettarinfo(str(f),arcname='treated-event.deadpan/'+str(f.relative_to(p)))
   item.uid=item.gid=item.mtime=0;item.uname=item.gname=''
   with f.open('rb') as data:archive.addfile(item,data)
(out/'treated-event.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(),mtime=0))
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files')
