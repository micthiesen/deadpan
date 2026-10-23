from pathlib import Path
import subprocess,os,time,json,hashlib
s=Path('/tmp/deadpan-neutral-ungroup-2026-10-22');b=s/'baseline';phase='baseline-pcm'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def sources():return {p:sha(b/p)for p in subprocess.check_output(['rg','--files','-g','*.rs'],cwd=b,text=True).splitlines()}
frozen=sources();(s/(phase+'-source-identities.json')).write_text(json.dumps(frozen,indent=2)+'\n')
inputs={p:sha(b/p)for p in ['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml','crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md']}
(s/(phase+'-inputs.json')).write_text(json.dumps(inputs,indent=2)+'\n')
for name in ['crates/deadpan-audio/tests/audio_definition/sound_clocks.rs','crates/deadpan-audio/tests/audio_definition/sound_clocks/neutral_ungroup.rs']:
 target=s/(phase+'-overlays')/(name+'.txt');target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes((b/name).read_bytes())
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',DEADPAN_SCOPE_PROBE_EVIDENCE=str(s/'baseline-pcm-evidence'))
def run(label,args):
 start=time.monotonic()
 with(s/f'{phase}-{label}.log').open('w')as log:r=subprocess.run(args,cwd=b,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'command':args,'exit':r.returncode,'seconds':time.monotonic()-start}
 (s/f'{phase}-{label}-receipt.json').write_text(json.dumps(row,indent=2)+'\n')
 assert sources()==frozen and all(sha(b/p)==v for p,v in inputs.items())
 print(label,json.dumps(row),flush=True)
 return r.returncode
archive=s/(phase+'.tar.zst');extract=s/(phase+'-extracted');extract.mkdir(exist_ok=True)
if archive.exists():
 assert json.loads((s/(phase+'-build-receipt.json')).read_text())['exit']==0
else:
 code=run('build',['cargo','nextest','archive','--locked','-p','deadpan-audio','--test','audio_definition','--target-dir',str(s/'baseline-target'),'--archive-file',str(archive),'--zstd-level=-1'])
 if code:raise SystemExit(code)
reuse=['--archive-file',str(archive),'--extract-to',str(extract),'--extract-overwrite','--workspace-remap',str(b)]
with(s/(phase+'-list.json')).open('w')as out:
 subprocess.run(['cargo','nextest','list',*reuse,'--message-format','json'],cwd=b,env=env,stdout=out,check=True)
bins={r['binary-path']:sha(Path(r['binary-path']))for r in json.loads((s/(phase+'-list.json')).read_text())['rust-suites'].values()}
(s/(phase+'-binaries.json')).write_text(json.dumps(bins,indent=2)+'\n')
code=run('run',['cargo','nextest','run',*reuse,'-E','test(neutral_ungroup)','--test-threads','2','--no-fail-fast'])
assert all(sha(Path(p))==v for p,v in bins.items())
print(json.dumps({'source_base':'c72abe12965b08f7705e0c9a61ab2298d5a562ae','exit':code,'sources_inputs_executables_unchanged':True}),flush=True)
raise SystemExit(code)
