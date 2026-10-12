import gzip,hashlib,json,shutil,subprocess
from pathlib import Path
s=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
out=repo/'tools/ui-feedback/evidence/2026-10-12-retained-viewer-pixels'
out.mkdir(parents=True,exist_ok=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(n,v):(out/n).write_text(json.dumps(v,indent=2)+'\n')
records=[]
for p in sorted(s.glob('*.json')):
 r=json.loads(p.read_text())
 if isinstance(r,dict) and 'log_sha256' in r:
  assert sha(s/(r['name']+'.log'))==r['log_sha256']
  records.append(r)
write('commands.json',records)
for p in sorted(s.glob('*.log')):(out/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
for n in ['README.md','review.md','inspect_archived.py','archived-pixels.json','run.py','checks.py','retain.py','check-evidence.py']:
 shutil.copyfile(s/n,out/n)
base='9b7df2d547375c8997ca72a214fe29150311dffc'
paths=['crates/deadpan-app/src/preview/harness/sound_placement.rs','crates/deadpan-app/src/preview/harness.rs','crates/deadpan-app/src/ui_harness/gpu.rs','crates/deadpan-app/src/preview.rs','Cargo.toml','Cargo.lock','rust-toolchain.toml']
write('identity.json',dict(base_commit=base,sources={p:sha(repo/p) for p in paths},binaries={'target/release/deadpan-app':sha(repo/'target/release/deadpan-app')}))
(out/'implementation.patch').write_bytes(subprocess.check_output(['git','diff',base,'--','crates'],cwd=repo))
for n in ['debug-pixel-replay','replay']:
 p=s/n/'report.json';(out/(n+'-report.json.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
r=json.loads((s/'replay/report.json').read_text());case=next(x for x in r['scenarios'] if x['name']=='sound-placement')
for size,n in [('960x640','minimum.png'),('1280x820','default.png')]:
 step=next(x for x in case['steps'] if x['input']=='Sound removal is reachable at '+size)
 shutil.copyfile(s/'replay'/step['screenshot'],out/n)
write('environment.json',dict(machine='Apple M5 Max',memory_bytes=137438953472,macos='26.5.2 (25F84)',rust='1.97.1',agents='primary only',live_gui=False,audio_device=False))
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files')
