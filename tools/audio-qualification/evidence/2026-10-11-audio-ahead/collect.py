from pathlib import Path
import gzip,hashlib,json,shutil,subprocess
scratch=Path('/tmp/deadpan-audio-ahead-20261011')
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/audio-qualification/evidence/2026-10-11-audio-ahead'
root.mkdir(parents=True,exist_ok=True)
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(name,data):(root/name).write_text(json.dumps(data,indent=2)+'\n')
def capture(*args):return subprocess.check_output(args,cwd=repo,text=True).strip()
for p in scratch.iterdir():
 if p.is_file() and p.suffix in ['.json','.py']:shutil.copyfile(p,root/p.name)
 elif p.is_file() and p.suffix in ['.log','.stderr']:(root/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
summary=[]
for name in ['diagnostics','original-playback','sound-playback']:
 folder=scratch/'replays'/name
 report=json.loads((folder/'report.json').read_text())
 (root/('replay-'+name+'.json.gz')).write_bytes(gzip.compress((folder/'report.json').read_bytes(),mtime=0))
 images=[]
 if name=='diagnostics':
  for step in [step for scenario in report['scenarios'] for step in scenario['steps'] if step['screenshot'] and step['input'] in ['Diagnostics beside the picture','Diagnostic export completed without user content']]:
   output='replay-'+name+'-'+step['screenshot'];shutil.copyfile(folder/step['screenshot'],root/output)
   images.append(dict(file=output,input=step['input']))
 summary.append(dict(scenario=name,failed=report['failed'],checks=sum(len(s['checks']) for s in report['scenarios']),binary_sha256=report['metadata']['binary_sha256'],selected_images=images,unmeasured=[reason for s in report['scenarios'] for reason in s['skipped']]))
save('replay-summary.json',summary)
shutil.copyfile(scratch/'replays'/'diagnostics'/'diagnostic-report.json',root/'exported-diagnostics.json')
save('host.json',dict(os=capture('sw_vers'),hardware=capture('sysctl','machdep.cpu.brand_string','hw.physicalcpu','hw.memsize'),rust=capture('rustc','-Vv'),qualified_ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'))
tracked=capture('git','diff','--name-only').splitlines();untracked=capture('git','ls-files','--others','--exclude-standard').splitlines()
paths=['Cargo.lock','rust-toolchain.toml']+sorted(set(p for p in tracked+untracked if p.startswith('crates/')))
save('source-pins.json',dict(base_revision=capture('git','rev-parse','HEAD'),files={p:sha(repo/p) for p in paths}))
diff=subprocess.check_output(['git','diff','--','crates'],cwd=repo)
for name in untracked:
 if name.startswith('crates/'):
  diff+=subprocess.run(['git','diff','--no-index','--','/dev/null',name],cwd=repo,stdout=subprocess.PIPE).stdout
(root/'reviewed-source.diff.gz').write_bytes(gzip.compress(diff,mtime=0))
save('manifest.json',{p.name:dict(bytes=p.stat().st_size,sha256=sha(p)) for p in sorted(root.iterdir()) if p.is_file() and p.name!='manifest.json'})
print(json.dumps(dict(evidence=str(root),replays=summary,bytes=sum(p.stat().st_size for p in root.iterdir() if p.is_file()))))
