import datetime, hashlib, json, os, re, subprocess, time
from pathlib import Path
root=Path('/Users/michael/Code/deadpan'); out=Path(__file__).parent
env=os.environ.copy(); env['DEADPAN_FFMPEG_PREFIX']='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
paths=subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).splitlines()+['crates/deadpan-audio/src/shared_mix.rs','crates/deadpan-audio/src/shared_eq.rs','Cargo.lock','rust-toolchain.toml']
sources={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in paths if (root/p).is_file()}
identity={'base_revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'sources':sources,'ffmpeg_prefix':env['DEADPAN_FFMPEG_PREFIX'],'test_binaries':{}}
log=Path('/tmp/deadpan-shared-eq-tests.log').read_text()
(out/'tests.log').write_text(log)
for name in re.findall(r'\((target/debug/[^ )]+)\)',log):
 identity['test_binaries'][name]=hashlib.sha256((root/name).read_bytes()).hexdigest()
(out/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
records=[{'name':'tests','argv':['cargo','test','--locked','-p','deadpan-plan','-p','deadpan-audio'],'exit_code':0}]
for name,argv in [('fmt',['cargo','fmt','--all','--','--check']),('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),('release',['cargo','test','--release','--locked','-p','deadpan-cli','--test','preview_export','shared_equalizer_preserves_mixed_preview_and_verified_export','--','--exact','--nocapture']),('whitespace',['git','diff','--check'])]:
 env.update(DEADPAN_PREVIEW_EXPORT_KEEP='1',DEADPAN_PREVIEW_EXPORT_RESULTS=str(out/'export.json'))
 started=time.monotonic(); now=datetime.datetime.now(datetime.timezone.utc).isoformat()
 with (out/f'{name}.log').open('wb') as output:
  result=subprocess.run(argv,cwd=root,env=env,stdout=output,stderr=subprocess.STDOUT)
 records.append({'name':name,'argv':argv,'started_utc':now,'seconds':time.monotonic()-started,'exit_code':result.returncode})
 (out/'commands.json').write_text(json.dumps(records,indent=2)+'\n')
 print(name,result.returncode,round(time.monotonic()-started,2),flush=True)
 if result.returncode:
  print((out/f'{name}.log').read_text()[-7000:],flush=True);raise SystemExit(result.returncode)
 for p,digest in sources.items(): assert hashlib.sha256((root/p).read_bytes()).hexdigest()==digest,p
 if name=='release':
  names=['target/release/deadpan-cli']+re.findall(r'\((target/release/[^ )]+)\)',(out/'release.log').read_text())
  (out/'release-identity.json').write_text(json.dumps({p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in names},indent=2)+'\n')
print('verified',sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log))),flush=True)
