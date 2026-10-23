from pathlib import Path
import subprocess,json,hashlib,time
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
manifest=json.loads((s/'release-media-binaries.json').read_text())
cli=manifest['helpers']['deadpan-cli'];binary=Path(cli['path'])
def sha(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def snapshot(package):
 return {str(f.relative_to(package)):{'bytes':f.stat().st_size,'sha256':sha(f)}for f in sorted(package.rglob('*'))if f.is_file()}
assert sha(binary)==cli['sha256']
out=s/'performance-validation';out.mkdir(exist_ok=False)
rows=[]
for i in range(1,6):
 package=s/'controlled-performance'/f'candidate-{i}.deadpan'
 before=snapshot(package);assert before
 cmd=[str(binary),'project','validate',str(package)]
 start=time.monotonic()
 with(out/f'candidate-{i}.stdout.json').open('w')as stdout,(out/f'candidate-{i}.stderr').open('w')as stderr:r=subprocess.run(cmd,stdout=stdout,stderr=stderr)
 assert r.returncode==0,(i,r.returncode)
 response=json.loads((out/f'candidate-{i}.stdout.json').read_text());assert response['valid']is True
 assert snapshot(package)==before and sha(binary)==cli['sha256']
 rows.append({'trial':i,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start,'response':response,'package_unchanged':True,'package_files':before})
 (out/'receipt.json').write_text(json.dumps({'cli':cli,'cases':rows},indent=2)+'\n')
subprocess.run(['python',str(s/'guard.py'),'release-perf'],check=True)
print(json.dumps({'valid_packages':len(rows),'all_paths_bytes_unchanged':True,'full_replay_no_quick_flag':True}))
